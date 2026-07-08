// v1.6 c3 — swizzle XCUIApplicationProcess waitForQuiescenceIncludingAnimationsIdle
// to early-return no-op, mirror maestro `cli-2.2.0`
// `XCUIApplicationProcess+FBQuiescence.m`. 真因: Apple `snapshot()` 内部触发
// `waitForQuiescenceIncludingAnimationsIdle:` 自动等 app idle (含动画结束),
// RN 持续动画 (loading spinner / CursorView) 致 wait 阻 5+s / timeout, dashboard
// 等 flow 在 smix 跑 fluky 1/3 PASS. maestro 同问题 fix = swizzle Apple
// quiescence wait → 直接 no-op, snapshot 即时 fire 当前 frame, race window
// 用 multi-step yaml + assertVisible 30s timeout 实际兜底.
//
// Selector 两 variant 都 swizzle (Apple iOS 17+ 后增 isPreEvent: 参数):
//   - waitForQuiescenceIncludingAnimationsIdle:
//   - waitForQuiescenceIncludingAnimationsIdle:isPreEvent:
//
// method_setImplementation (跟 SmixA11ySwizzle.m 同 pattern, 不 exchange) +
// 不调 original = 永久 no-op. 不依赖 env / 不依赖 host 透传.

#import "SmixQuiescenceSwizzle.h"
#import <objc/runtime.h>

static void swizzledWaitForQuiescenceIncludingAnimationsIdle(id self, SEL _cmd, BOOL includingAnimations) {
    // no-op — let snapshot/event 立即 fire (maestro `cli-2.2.0` 同源)
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
