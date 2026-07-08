// v1.6 c2 — modal subtree access via ObjC category `+load` swizzle.
// 跟 maestro `cli-2.2.0` `XCAXClient_iOS+FBSnapshotReqParams.h` 1:1 同源.
// modal overlay `snapshotKeyHonorModalViews=NO` 由 `+load` 在 dyld 阶段
// 永久装 (早于 XCTest framework 初始化, 早于第一次 a11y snapshot).

#import <Foundation/Foundation.h>

NS_ASSUME_NONNULL_BEGIN

/// Custom snapshot parameter API. Parallel to maestro
/// `FBSetCustomParameterForElementSnapshot` — Swift `AXClientSwizzler`
/// maxDepth fallback path (v1.6+) 通过此 setter 注入.
void SmixSetCustomA11yParameter(NSString *name, id value);

id _Nullable SmixGetCustomA11yParameter(NSString *name);

NS_ASSUME_NONNULL_END
