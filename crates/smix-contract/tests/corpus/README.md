# A corpus with the shape of a real app

These contracts were written by reading a real two-platform product's
test suites and flows, and they carry that product's *shape* rather than
its text: the source is confidential, and this crate is published.

What the reading found, and why it is the argument for this crate:

**The two native suites already write the same sentence.** A menu's
separator rules are covered on both platforms, and the test names on
each side are word-for-word the same English. Two suites, one
requirement, one wording — and nothing can join them, because the
sentence has no id. That is not a gap in discipline; it is a missing
identity.

**One suite has no counterpart.** A callout that flips above its anchor
when there is no room below is covered on one platform and not the
other. Nothing in either repository says so. It is the kind of fact
that is obvious once asked and unaskable until now.

The corpus deliberately exercises all three sets — something nobody
claims, something one platform claims, something both claim. A corpus
where everything is fully claimed would leave the other two answers
untested against realistic data, which is the same mistake as a gate
that only ever drives the easy subject.
