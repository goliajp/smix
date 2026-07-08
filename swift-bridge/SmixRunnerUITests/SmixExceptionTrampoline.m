#import "SmixExceptionTrampoline.h"

NSString * const SmixExceptionTrampolineErrorDomain =
  @"dev.smix.SmixExceptionTrampoline";

static NSError *SmixErrorFromException(NSException *exception) {
  NSString *reason = exception.reason ?: @"";
  NSString *name = exception.name ?: @"NSException";
  NSString *message = [NSString stringWithFormat:@"%@: %@", name, reason];
  return [NSError errorWithDomain:SmixExceptionTrampolineErrorDomain
                             code:1
                         userInfo:@{
    NSLocalizedDescriptionKey: message,
    @"exceptionName": name,
    @"exceptionReason": reason,
  }];
}

BOOL SmixRunCatching(void (NS_NOESCAPE ^block)(void),
                      NSError * _Nullable * _Nullable error) {
  __block BOOL ok = YES;
  __block NSError *caught = nil;

  // The @try/@catch lives INSIDE the main-queue work item so a vanished-
  // element NSException is caught on the SAME thread it is thrown from
  // (the main thread). Catching across the dispatch_sync barrier would be
  // undefined; this keeps exception handling local to the throwing frame.
  void (^guardedOnMain)(void) = ^{
    @try {
      block();
    } @catch (NSException *exception) {
      ok = NO;
      caught = SmixErrorFromException(exception);
    }
  };

  if ([NSThread isMainThread]) {
    // Deadlock guard: a dispatch_sync onto the main queue from the main
    // thread would deadlock. FlyingFox route closures never run on main,
    // so this branch is defensive — run inline.
    guardedOnMain();
  } else {
    // FlyingFox handlers run on the cooperative pool (off-main). The
    // runner's test_runForever lives on XCTest's background test thread,
    // so the main run loop is alive and services this work item — no
    // deadlock (the calling thread is provably not the main thread).
    dispatch_sync(dispatch_get_main_queue(), guardedOnMain);
  }

  if (!ok && error != NULL) {
    *error = caught;
  }
  return ok;
}
