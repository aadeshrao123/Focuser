# Focuser — Privacy Policy

**Last updated:** April 4, 2026

## Overview

Focuser is an open-source website and application blocker. The Focuser browser extension works with the Focuser desktop application to block distracting websites. This privacy policy explains what data the extension accesses and how it is handled.

## Data We Access

### Web History (Tab URLs)

The extension reads the URLs of your browser tabs to determine whether a website should be blocked based on your configured block lists. This is the core functionality of the extension.

- URLs are checked **locally in your browser** against your block list rules
- URLs are **never sent to any external server**
- URLs are **never stored, logged, or recorded** by the extension
- URL checking happens entirely within the extension's background process

### Local Communication

The extension communicates with the Focuser desktop application installed on your computer using Chrome's Native Messaging API. This communication is:

- **Strictly local** — between the extension and the desktop app on your machine
- Used to sync blocking rules from the desktop app to the extension
- Used to report blocked attempt counts back to the desktop app for local statistics
- **No data leaves your device** through this channel

## Data We Do NOT Collect

- No personally identifiable information
- No browsing history or browsing patterns
- No cookies or session data
- No form data or passwords
- No financial information
- No health information
- No location data
- No user activity tracking (clicks, scrolls, keystrokes)
- No website content (text, images, videos)

## Data Storage

All blocking rules and statistics are stored locally on your computer in the Focuser desktop application's database. The extension itself only caches the current set of blocking rules in local browser storage for performance.

## Data Sharing

We do not sell, transfer, or share any user data with third parties. Period.

- No analytics services
- No advertising networks
- No data brokers
- No external APIs

## Third-Party Services

The extension does not connect to any third-party services. All functionality is local to your device.

## Open Source

Focuser is fully open source. You can inspect the complete source code at:

**https://github.com/aadeshrao123/Focuser**

## Changes to This Policy

If we make changes to this privacy policy, we will update the "Last updated" date above. Significant changes will be noted in the extension's changelog.

## Contact

If you have questions about this privacy policy, please open an issue on our GitHub repository:

**https://github.com/aadeshrao123/Focuser/issues**
