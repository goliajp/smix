// Disables Apple's automatic quiescence wait, which otherwise stalls on
// apps with continuous animation. Mirrors maestro `cli-2.2.0`'s
// `XCUIApplicationProcess+FBQuiescence.m` (where FBWaitForIdleTimeout=0
// defaults to an effective no-op return).
//
// Header-only declaration; the implementation lives entirely in the .m
// behind `+load`.

#import <Foundation/Foundation.h>

NS_ASSUME_NONNULL_BEGIN

NS_ASSUME_NONNULL_END
