// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors
//
// Process-held cache locking and background resource budgets, independent of a graphics context.
#include <algorithm>
#include <fstream>
#include <stdexcept>
#include <string>
#include <thread>

#include "enhanced/tools/baker/pipeline/pipeline_internal.h"

#if defined(_WIN32)
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <fcntl.h>
#include <io.h>
#include <process.h>
#include <share.h>
#include <windows.h>

#include <sys/locking.h>
#include <sys/stat.h>
#else
#include <fcntl.h>

#include <sys/file.h>
#include <sys/resource.h>
#if defined(__APPLE__)
#include <sys/sysctl.h>
#endif
#include <unistd.h>
#endif

namespace openrac::lighting::pipeline {
namespace {
std::uint64_t process_id() {
#if defined(_WIN32)
    return static_cast<std::uint64_t>(_getpid());
#else
    return static_cast<std::uint64_t>(getpid());
#endif
}

void write_lock_owner(int file, const std::filesystem::path& directory) {
    const std::string text =
        "pid=" + std::to_string(process_id()) + "\ncache=" + directory.string() + "\n";
#if defined(_WIN32)
    if (_lseek(file, 0, SEEK_SET) >= 0 && _chsize_s(file, 0) == 0) {
        (void)_write(file, text.data(), static_cast<unsigned>(text.size()));
        (void)_commit(file);
    }
#else
    if (lseek(file, 0, SEEK_SET) >= 0 && ftruncate(file, 0) == 0) {
        (void)write(file, text.data(), text.size());
        (void)fsync(file);
    }
#endif
}

std::string lock_owner(const std::filesystem::path& path) {
    std::ifstream stream(path, std::ios::binary);
    std::string owner((std::istreambuf_iterator<char>(stream)), std::istreambuf_iterator<char>());
    while (!owner.empty() && (owner.back() == '\n' || owner.back() == '\r')) {
        owner.pop_back();
    }
    return owner;
}

// Queries physical RAM for the scene budget; a failed query uses the conservative job fallback.
std::uint64_t physical_memory() {
#if defined(_WIN32)
    MEMORYSTATUSEX status{};
    status.dwLength = sizeof(status);
    return GlobalMemoryStatusEx(&status) ? status.ullTotalPhys : 0;
#elif defined(__APPLE__)
    std::uint64_t bytes = 0;
    std::size_t size = sizeof(bytes);
    return sysctlbyname("hw.memsize", &bytes, &size, nullptr, 0) == 0 ? bytes : 0;
#else
    const std::int64_t pages = sysconf(_SC_PHYS_PAGES);
    const std::int64_t size = sysconf(_SC_PAGESIZE);
    return pages > 0 && size > 0
               ? static_cast<std::uint64_t>(pages) * static_cast<std::uint64_t>(size)
               : 0;
#endif
}
}  // namespace

void background(Options& options) {
    const unsigned cpus = std::max(1u, std::thread::hardware_concurrency());

    // Reserve an eighth of the cores, at least two, for the desktop.
    if (!options.threads) {
        options.threads =
            std::min(256u, std::max(1u, cpus - std::min(cpus - 1, std::max(2u, cpus / 8))));
    }

    // Automatic concurrency matches one level per four tracing workers.
    if (!options.jobs) {
        options.jobs = std::max(1u, options.threads / 4);
    }

    const auto ram = physical_memory();

    // Half physical RAM is the default scene budget; unknown RAM keeps a conservative GiB per job.
    if (!options.memory) {
        options.memory =
            std::max(std::uint64_t{1} << 30, ram ? ram / 2 : (std::uint64_t{options.jobs} << 30));
    }
#if defined(_WIN32)
    // Priority is best effort; a denied adjustment must not prevent a valid bake.
    SetPriorityClass(GetCurrentProcess(), BELOW_NORMAL_PRIORITY_CLASS);
#else
    // Shared tracing workers inherit this controller priority.
    setpriority(PRIO_PROCESS, 0, 10);
#endif
}

CacheLock::CacheLock(const std::filesystem::path& directory) {
    const auto path = directory / "probe-bake.lock";
#if defined(_WIN32)
    const int opened = _wsopen_s(
        &m_file,
        path.c_str(),
        _O_CREAT | _O_RDWR | _O_BINARY | _O_NOINHERIT,
        _SH_DENYNO,
        _S_IREAD | _S_IWRITE
    );

    // A failed open must not pass an invalid descriptor to the locking API.
    if (opened != 0) {
        m_file = -1;
    }

    // Lock the same first byte used by the Python reference flow.
    if (m_file >= 0 && _locking(m_file, _LK_NBLCK, 1) == 0) {
        write_lock_owner(m_file, directory);
        return;
    }

    // A failed Windows lock still owns a descriptor that must be closed.
    if (m_file >= 0) {
        _close(m_file);
    }
#else
    // Ordinary cache permissions still respect the user's process umask.
    m_file = open(path.c_str(), O_CREAT | O_RDWR | O_CLOEXEC, 0666);

    // POSIX uses the same process-held lock as the Python reference flow.
    if (m_file >= 0 && flock(m_file, LOCK_EX | LOCK_NB) == 0) {
        write_lock_owner(m_file, directory);
        return;
    }

    // A failed POSIX lock must close its newly opened descriptor.
    if (m_file >= 0) {
        close(m_file);
    }
#endif
    m_file = -1;
    const auto owner = lock_owner(path);
    throw std::runtime_error(
        "bake cache is busy or cannot be locked: " + path.string()
        + (owner.empty() ? std::string{} : " (" + owner + ")")
    );
}

CacheLock::~CacheLock() {
    // Closing the owned descriptor releases locks on completion, exception or cancellation.
    if (m_file >= 0) {
#if defined(_WIN32)
        _close(m_file);
#else
        close(m_file);
#endif
    }
}
}  // namespace openrac::lighting::pipeline

namespace openrac::lighting {
bool bake_cache_busy(const std::filesystem::path& directory) {
    if (!std::filesystem::is_directory(directory)) {
        return false;
    }
    try {
        // This is the exact lock a bake would take. Destruction releases it immediately when idle.
        pipeline::CacheLock lock(directory);
        return false;
    } catch (const std::runtime_error&) {
        return true;
    }
}
}  // namespace openrac::lighting
