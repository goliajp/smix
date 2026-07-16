// Modal subtree access via an ObjC `+load` swizzle, mirroring maestro
// `cli-2.2.0`'s `XCAXClient_iOS+FBSnapshotReqParams.h`. The overlay
// `snapshotKeyHonorModalViews=NO` is installed permanently from `+load`
// during the dyld phase — before the XCTest framework initializes and
// before the first a11y snapshot is taken.

#import <Foundation/Foundation.h>

NS_ASSUME_NONNULL_BEGIN

/// Custom snapshot parameter API. Parallel to maestro
/// `FBSetCustomParameterForElementSnapshot`. The Swift `AXClientSwizzler`
/// maxDepth fallback path injects through this setter.
void SmixSetCustomA11yParameter(NSString *name, id value);

id _Nullable SmixGetCustomA11yParameter(NSString *name);

NS_ASSUME_NONNULL_END
