// v1.6 c3 — disable Apple auto-quiescence wait for RN apps with continuous
// animation. 跟 maestro `cli-2.2.0` `XCUIApplicationProcess+FBQuiescence.m`
// 同源 (FBWaitForIdleTimeout=0 默认即 effective no-op return).
//
// Header-only declaration; implementation 全在 .m + `+load`.

#import <Foundation/Foundation.h>

NS_ASSUME_NONNULL_BEGIN

NS_ASSUME_NONNULL_END
