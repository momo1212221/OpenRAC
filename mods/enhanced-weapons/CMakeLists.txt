cmake_minimum_required(VERSION 3.20)
project(enhanced_weapons LANGUAGES C CXX)

# Standalone repository: fetch the SDK pinned by sdk.lock with tools/fetch_sdk.py.
# Inside an OpenRAC checkout, use its SDK directly for development.
if(NOT OpenRACMod_DIR)
  if(DEFINED OPENRAC_SDK)
    set(OpenRACMod_DIR "${OPENRAC_SDK}/cmake")
  elseif(EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/../../sdk/cmake/OpenRACModConfig.cmake")
    set(OpenRACMod_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../sdk/cmake")
  elseif(EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/sdk.lock")
    file(READ "${CMAKE_CURRENT_SOURCE_DIR}/sdk.lock" lock)
    string(JSON sdk_version GET "${lock}" version)
    set(OpenRACMod_DIR "${CMAKE_CURRENT_SOURCE_DIR}/.sdk/openrac-mod-sdk-${sdk_version}/cmake")
    if(NOT EXISTS "${OpenRACMod_DIR}/OpenRACModConfig.cmake")
      message(FATAL_ERROR "OpenRAC SDK missing: run python3 tools/fetch_sdk.py")
    endif()
  endif()
endif()

find_package(OpenRACMod 1.14 REQUIRED)
openrac_add_mod(enhanced_weapons MANIFEST openrac-mod.json SOURCES src/mod.cpp)
