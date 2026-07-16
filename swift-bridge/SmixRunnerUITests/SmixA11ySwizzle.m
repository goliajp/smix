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
      // Capture the originals on the first real call (i.e. after the XCTest
      // framework has initialized) rather than at +load time: at +load
      // XCAXClient_iOS has not yet finalized Apple's internal defaults.
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
    // Runs during the dyld phase, before the XCTest framework initializes.
    // NSClassFromString looks the class up dynamically: if XCAXClient_iOS is
    // not registered yet (XCTAutomationSupport.dylib not loaded), this
    // silently skips without breaking anything. In practice dyld's +load
    // ordering guarantees the class exists by the time this runs, because a
    // UITest target always links XCTAutomationSupport.

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
