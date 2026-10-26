# Missing-extension installation window

The Windows session agent prefers the existing journalled installation-only
browser policies. Those recovery browsers remain open for installing the
extension and leave recovery after a fresh covered extension check-in.

If policy installation is unavailable (for example, an unsupported browser or a
conflicting managed policy), a persisted, expired service grace deadline used
to close every newly opened browser immediately. That prevented installation.

The agent now permits a manual installation attempt for 60 seconds from when
it observes a new browser main process. Failed policy installation, including
reapplying interrupted recovery, uses this same deadline. Polling, retrying a
policy write, or adding another main process while an existing one is running
does not renew the attempt. Once the uncovered browser exits completely, a
reopened browser receives another attempt, even if the ordinary service grace
deadline has already expired. Browsers have independent attempts.

This fallback intentionally allows ordinary browser access for that bounded
installation period when installation-only policies cannot be applied. It does
not automatically launch an unrestricted browser. After the minute, the existing
browser-close action applies again if coverage is still missing. The warning
explains how to install/enable the extension and reopen for another attempt.

Restored extension coverage cancels the normal missing-extension close path.
Fresh recovery-ready coverage also restores journalled policies before retrying
an interrupted recovery, so a policy reapplication error cannot trap a browser
after its extension has reconnected. Stopping the active blocking period clears
attempt state through the existing policy-restoration path.

The service still owns the active-policy decision, stored grace deadlines,
commitments and database. No IPC authorization, uninstall protection or browser
extension code is changed. A restarted agent starts observing process lifetimes
again; these short manual-installation attempts are not durable commitments.

Automated coverage exercises exact deadline expiry, fresh reopening after
expiry, repeated failures without renewal, overlapping main-process lifetimes,
and independent browsers. All 22 agent tests passed in the optimized Windows
build, including recovery policy rollback, process selection, API framing and
startup/reconnect behavior.

Direct reinstall exposed an unapplied-policy cleanup case: if policy creation
failed, the journal could record an originally absent key that remained absent.
Restoration now leaves that key absent instead of requiring write permission to
create and delete it. A scratch registry test explicitly denies subkey creation;
the full agent suite now passes 23 tests. Existing original-value restoration
and third-party-conflict preservation remain covered.

Manual acceptance remains necessary on Windows with a blocking session: test a
policy-installation failure, wait for closure, reopen, install within the next
minute, and confirm that fresh coverage stops closure. Also test the normal
installation-only recovery path and stopping the blocking session. Do not remove
the user's actual extension or activate persistent enforcement just to run these
checks on the development machine.

## Recognition after reinstalling

Brave's installed profile on the affected machine contains a complete extension
record but omits both `state` and `disable_reasons`. Previously this was treated
as unknown, which kept a preceding confirmed-removal latch set forever. The
monitor now recognizes complete installed metadata in this format; explicit
disabled states and disable reasons still take precedence. Empty, malformed or
unreadable records do not clear a confirmed removal. Directory disappearance
still detects another removal immediately. Chromium also defaults absent disable
reasons to an empty set in its [preference implementation](https://chromium.googlesource.com/chromium/src/+/master/extensions/browser/extension_prefs.cc).

A covered, fresh recovery check-in now restores journalled policies and cancels
pending restarts and installation deadlines without automatically closing the
browser. Failed restoration retains the journal for retry and does not fall
through to a browser close. Existing service-owned active-block checks and
private-window coverage requirements remain in place. Browser extension files
are untouched.

Regression results for this correction: all 8 focused extension-monitor tests
and all 25 optimized Windows session-agent tests passed. The new cases cover
modern installed metadata with omitted legacy fields, disabled/malformed
records, removal/reinstallation/removal transitions, an expired installation
timer with a pending restart, and failed policy restoration followed by retry.
