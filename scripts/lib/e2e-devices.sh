#!/usr/bin/env bash
# The devices the e2e scripts drive when their caller names none.
#
# Source this, do not run it. Each script still takes its own override
# variable (SMIX_C5_ANDROID, SMIX_SMOKE_IOS, …) — that is how a caller points
# one script elsewhere. What lives here is the default, once: nineteen
# scripts used to write `sim-smix-android-01` into themselves, so moving the
# suite to another device was nineteen edits, and the release had to export
# nine variables to keep them off an AVD somebody else was using.
#
# SMIX_E2E_ANDROID / SMIX_E2E_IOS point the whole suite at once.
# E2E_ANDROID_SECOND is the other smix AVD, for the scripts whose subject
# is two emulators. E2E_IOS_SECOND is the iOS counterpart: `sim-smix-03`,
# by UDID because it is not registered — registering it would write to
# this machine's device registry, which consumers' rows share.
E2E_ANDROID="${SMIX_E2E_ANDROID:-sim-smix-android-01}"
E2E_ANDROID_SECOND="${SMIX_E2E_ANDROID_SECOND:-sim-smix-android-02}"
E2E_IOS="${SMIX_E2E_IOS:-sim-smix-02}"
E2E_IOS_SECOND="${SMIX_E2E_IOS_SECOND:-89980B43-EF26-446A-A897-848C1AD3A872}"
