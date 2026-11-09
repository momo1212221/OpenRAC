# Changelog

## 0.3.2

- Checks the loaded-mod list immediately before applying XP or tier damage.
- Prevents even the first post-load hit from being double-scaled when Arsenal Mastery is active.
- Keeps the periodic scan as a low-cost status/logging fallback.

## 0.3.1

- Detects Arsenal Mastery through the OpenRAC 1.14 loaded-mod list.
- Automatically disables Enhanced Weapons' own XP and tier-damage progression while Arsenal Mastery is active.
- Keeps hit markers, projectile heat haze and Enhanced RAC visual effects active.
- Prevents double XP/damage scaling when both personal-list mods are installed.

## 0.3.0

- Replaced plain weapon XP HUD text with the native OpenRAC 1.14 overlay renderer.
- Added Enhanced RAC Style weapon card with gold/orange tech framing.
- Added segmented V1-V5/OMEGA progression strip and animated XP bar.
- Added dedicated tier badge with special OMEGA treatment.
- Added centred animated weapon-upgrade banner.
- Replaced the basic text hit marker with a stylised in-game cross marker.
- Raised the required OpenRAC mod interface to 1.14.

## 0.2.0

- Added persistent per-save weapon XP.
- Added V1, V2, V3, V4, V5 and OMEGA weapon tiers.
- Added tier HUD with current XP and next threshold.
- Added level-up notifications.
- Added configurable XP rate.
- Added real tier damage bonuses from +8% at V2 to +65% at OMEGA.
- Added small tier-based projectile heat-haze scaling.
- Progress now covers 15 RAC1 combat weapons.
- Changed multiplayer classification from cosmetic to gameplay.

## 0.1.0

- Initial release.
- Configurable hit marker.
- Optional heat distortion around known player projectiles.
- PAL and NTSC-U support.
