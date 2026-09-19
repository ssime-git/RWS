# Explicit remote agent launch

The user wants agents installed on the VM to execute there, for any workspace.
An SSH non-login command currently cannot find the user's agent executables.

1. Add `rws agent --workspace NAME -- EXECUTABLE [ARGS]`: always SSH, PTY by
   default, optional no-PTY acceptance mode; remote login environment, quoted
   arguments, change directory after profile loading, remote identity banner.
2. Add an app executable field and launch button. Generate a private `.command`
   that execs the bundled RWS command in Terminal. No local agent fallback; do not
   forward credentials or install agents. Shell window is local, agent is remote.
3. Test quoting, missing agent, failed SSH, login environment, remote process
   identity and real installed agent versions. No paid inference needed.
4. Build the app and document limits: independent local IDEs are not intercepted,
   agent authentication/inference and reconnect persistence are separate checks.
