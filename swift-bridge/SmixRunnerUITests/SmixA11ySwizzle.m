// ObjC `+load` swizzle of `XCAXClient_iOS.defaultParameters` and
// `XCTElementQuery.snapshotParameters`. Mechanics:
//   - method_setImplementation (not exchange); the original IMP is
//     stashed in a C function pointer
//   - the swizzled IMP calls the original via C ABI (no ObjC msgSend)
//   - a dispatch_once captures the default request parameters (Apple's
//     5-key baseline)
//
// The overlay hard-codes `snapshotKeyHonorModalViews=@0` so modal
// subtrees (drawers, dialogs) are traversable; Apple's default of YES
// otherwise excludes them from the snapshot.

#import "SmixA11ySwizzle.h"
#import <objc/runtime.h>

static id (*original_defaultParameters)(id, SEL);
static id (*original_snapshotParameters)(id, SEL);
static NSDictionary *defaultRequestParameters;
static NSDictionary *defaultAdditionalRequestParameters;
static NSMutableDictionary *customRequestParameters;

void SmixSetCustomA11yParameter(NSString *name, id value) {
    static dispatch_once_t onceToken;
    dispatch_once(&onceToken, ^{
      customRequestParameters = [NSMutableDictionary new];
    });
    customRequestParameters[name] = value;
}

id SmixGetCustomA11yParameter(NSString *name) {
    return customRequestParameters[name];
}

static id swizzledDefaultParameters(id self, SEL _cmd) {
    static dispatch_once_t onceToken;
    dispatch_once(&onceToken, ^{
      // original 只在第一次实际调用时 capture (XCTest framework 初始化后),
      // 不是 +load 时 — 此时 XCAXClient_iOS 内部 Apple 默认还没 finalize.
      defaultRequestParameters = original_defaultParameters(self, _cmd);
    });
    NSMutableDictionary *result =
        [NSMutableDictionary dictionaryWithDictionary:defaultRequestParameters];
    [result addEntriesFromDictionary:defaultAdditionalRequestParameters ?: @{}];
    [result addEntriesFromDictionary:customRequestParameters ?: @{}];
    return result.copy;
}

static id swizzledSnapshotParameters(id self, SEL _cmd) {
    NSDictionary *result = original_snapshotParameters(self, _cmd);
    defaultAdditionalRequestParameters = result;
    return result;
}

@interface SmixA11ySwizzleInstaller : NSObject
@end

@implementation SmixA11ySwizzleInstaller

#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wobjc-load-method"
#pragma clang diagnostic ignored "-Wcast-function-type-strict"

+ (void)load {
    // dyld 阶段触发, 早于 XCTest framework 初始化. NSClassFromString 动态查 —
    // 如 XCAXClient_iOS 尚未注册 (XCTAutomationSupport.dylib 未 load), silent
    // skip 不破坏 (后续 +load order 由 dyld 保证: XCTest UITest target 必 link
    // XCTAutomationSupport, 此 +load 跑时 class 已存).

    SmixSetCustomA11yParameter(@"snapshotKeyHonorModalViews", @0);

    Class axClass = NSClassFromString(@"XCAXClient_iOS");
    if (axClass != Nil) {
        Method m = class_getInstanceMethod(axClass, NSSelectorFromString(@"defaultParameters"));
        if (m != NULL) {
            IMP imp = (IMP)swizzledDefaultParameters;
            original_defaultParameters = (id(*)(id, SEL))method_setImplementation(m, imp);
            NSLog(@"smix-runner: SmixA11ySwizzle: defaultParameters swizzled");
        } else {
            NSLog(@"[ERROR] smix-runner: SmixA11ySwizzle: defaultParameters method not found");
        }
    } else {
        NSLog(@"[ERROR] smix-runner: SmixA11ySwizzle: XCAXClient_iOS class not found");
    }

    Class queryClass = NSClassFromString(@"XCTElementQuery");
    if (queryClass != Nil) {
        Method m = class_getInstanceMethod(queryClass, NSSelectorFromString(@"snapshotParameters"));
        if (m != NULL) {
            IMP imp = (IMP)swizzledSnapshotParameters;
            original_snapshotParameters = (id(*)(id, SEL))method_setImplementation(m, imp);
            NSLog(@"smix-runner: SmixA11ySwizzle: snapshotParameters swizzled");
        }
    }
}

#pragma clang diagnostic pop

@end
