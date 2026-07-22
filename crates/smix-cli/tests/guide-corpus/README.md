# The directory a guide example thinks it is running in

`08-cookbook` shows `runFlow` twice with two different layouts — once
as `../subflows/login.yaml`, once as `./subflows/launch-fresh.yaml` —
so a single tree that satisfies both needs a `subflows/` beside the
flow directory and another inside it. That is why there are two.

`flows/` is the base directory the gate runs examples from. Files under
it exist because a printed example names them; where the guide also
prints their contents, the file holds exactly that.
