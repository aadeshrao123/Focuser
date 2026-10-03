#!/bin/sh
# Runs as root, after the package is unpacked. Every step here is best-effort
# and must never fail the install over an optional nicety — a missing
# capability or unit just means the manual fallback (documented in the
# README) still applies.
set -e

# Without this, focuser-ui runs as a plain user and cannot write /etc/hosts —
# every reinstall (a fresh copy of the binary) needs it reapplied, since the
# capability lives on the file, not the package.
if command -v setcap >/dev/null 2>&1; then
    setcap cap_dac_override=+ep /usr/bin/focuser-ui || true
fi

if command -v systemctl >/dev/null 2>&1; then
    systemctl daemon-reload >/dev/null 2>&1 || true

    # The auto-respawn unit is a --user unit (no root needed to run Focuser
    # itself), so it has to be enabled inside the invoking user's own systemd
    # session, not root's. SUDO_USER is the closest thing dpkg gives us to
    # "who actually ran this install" — if it is unset (installed some other
    # way) or there is no active user session yet, this just does nothing;
    # the user can always run the same two commands themselves.
    if [ -n "${SUDO_USER:-}" ] && command -v runuser >/dev/null 2>&1; then
        runuser -l "$SUDO_USER" -c \
            'systemctl --user daemon-reload && systemctl --user enable --now focuser.service' \
            >/dev/null 2>&1 || true
    fi
fi

exit 0
