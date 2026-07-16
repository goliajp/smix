// Swizzles XCUIApplicationProcess's waitForQuiescenceIncludingAnimationsIdle
// to an early-returning no-op, mirroring maestro `cli-2.2.0`'s
// `XCUIApplicationProcess+FBQuiescence.m`.
//
// Why: Apple's `snapshot()` internally calls
// `waitForQuiescenceIncludingAnimationsIdle:`, which waits for the app to go
// idle including all animations finishing. An app with a continuously
// animating element (a loading spinner, a blinking cursor view) never
// reaches that state, so the wait blocks for 5+ seconds or times out and
// flows become flaky. maestro fixes the same problem the same way: no-op the
// quiescence wait so snapshot fires immediately against the current frame.
// The resulting race window is covered instead by explicit multi-step waits
// and assertVisible timeouts at the flow level.
//
// Both selector variants are swizzled (iOS 17+ added the isPreEvent: arg):
//   - waitForQuiescenceIncludingAnimationsIdle:
//   - waitForQuiescenceIncludingAnimationsIdle:isPreEvent:
//
// method_setImplementation (same pattern as SmixA11ySwizzle.m — not
// exchange) and never calling the original makes this a permanent no-op. It
// depends on no env var and on nothing passed through from the host.

#import "SmixQuiescenceSwizzle.h"
#import <objc/runtime.h>

static void swizzledWaitForQuiescenceIncludingAnimationsIdle(id self, SEL _cmd, BOOL includingAnimations) {
    // no-op — let the snapshot / event fire immediately
    return;
}

static void swizzledWaitForQuiescenceIncludingAnimationsIdlePreEvent(id self, SEL _cmd, BOOL includingAnimations, BOOL isPreEvent) {
    // no-op — iOS 17+ variant
    return;
}

@interface SmixQuiescenceSwizzleInstaller : NSObject
@end

@implementation SmixQuiescenceSwizzleInstaller

#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wobjc-load-method"
#pragma clang diagnostic ignored "-Wcast-function-type-strict"

+ (void)load {
    Class procClass = NSClassFromString(@"XCUIApplicationProcess");
    if (procClass == Nil) {
        NSLog(@"[ERROR] smix-runner: SmixQuiescenceSwizzle: XCUIApplicationProcess class not found");
        return;
    }

    // Try the iOS 17+ variant first (includes isPreEvent: arg). If not present,
    // fall back to single-arg legacy.
    SEL preEventSel = NSSelectorFromString(@"waitForQuiescenceIncludingAnimationsIdle:isPreEvent:");
    SEL legacySel = NSSelectorFromString(@"waitForQuiescenceIncludingAnimationsIdle:");

    Method preEventMethod = class_getInstanceMethod(procClass, preEventSel);
    Method legacyMethod = class_getInstanceMethod(procClass, legacySel);

    BOOL swizzled = NO;

    if (preEventMethod != NULL) {
        IMP imp = (IMP)swizzledWaitForQuiescenceIncludingAnimationsIdlePreEvent;
        method_setImplementation(preEventMethod, imp);
        NSLog(@"smix-runner: SmixQuiescenceSwizzle: waitForQuiescence(preEvent variant) swizzled to no-op");
        swizzled = YES;
    }

    if (legacyMethod != NULL) {
        IMP imp = (IMP)swizzledWaitForQuiescenceIncludingAnimationsIdle;
        method_setImplementation(legacyMethod, imp);
        NSLog(@"smix-runner: SmixQuiescenceSwizzle: waitForQuiescence(legacy variant) swizzled to no-op");
        swizzled = YES;
    }

    if (!swizzled) {
        NSLog(@"[ERROR] smix-runner: SmixQuiescenceSwizzle: neither waitForQuiescence variant found");
    }
}

#pragma clang diagnostic pop

@end
