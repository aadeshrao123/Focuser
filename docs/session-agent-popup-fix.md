# Repeated authority-disconnected dialogs — 2026-10-22

The session agent displayed a modal MessageBox on the first failed observation
or snapshot request. Any subsequent successful poll reset the notification
latch, so intermittent request failures could accumulate multiple dialogs.
This path did not check whether any block was active. The installed authority
was running and reported zero active lists during diagnosis.

The background poll loop now logs errors and continues reconnecting without
opening a dialog. The desktop's existing health banner continues to report an
unavailable authority or missing session agent. Pomodoro, allowance and browser
coverage notifications retain their existing behavior.

Validation: 32 optimized Windows session-agent tests and 7 frontend health-banner
tests passed. Targeted rustfmt passed. Only the agent's poll-error notification
path changed in source.

The installed agent was backed up and replaced while blocking and file protection
were idle. Its existing least-privilege scheduled task was stopped and restarted.
The service remained running with PID 4888. All other executable hashes and the
serialized block-list fingerprint matched before and after repair. The new agent
started from Program Files and reported connected.

Installed agent SHA-256:
`9CB4267778DBCE84C00680F57DA38B38EA2F0433A08647AEC91A987182BE7881`

Original binary backup:
`C:\ProgramData\Focuser.AgentRecovery-cd4b5fe83bfd4e9da9e6c9734e12e7ab`

Operator script and report: `target/session-agent-popup-fix/`.
Existing installer executables were not rebuilt for this local agent repair.
No service outage was deliberately induced on the installed system; this repair
does not establish the cause of the original intermittent transport failures.
