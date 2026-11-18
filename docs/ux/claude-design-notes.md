# UI proposals (Claude), 2026-11-18

Figma: https://www.figma.com/design/y0Yg98WyghpmV8wiQRkQJG (3 directions x 3 screens, plus icon concepts).
Mockups use Inter as a stand-in for SF Pro, which the Figma renderer could not draw.

## Why the current app feels off
- **It shows how the app works instead of what the user decides.** "Grant", "policy", "reservation", "payload" and raw JSON come first. Users decide two things: *who* is asking and *how much*. Those should be the biggest things on screen.
- **Everything is a card at the same weight.** Navy, teal and cream cards everywhere mean nothing stands out, including the one request waiting for you.
- **Developer controls sit next to user controls.** Server URL, demo data and password sign-in share a level with notifications. Most users never need them.
- **The icon** is an off-center key that can read as the male gender symbol. It says nothing about approval.

## The three directions
| | A. Wallet | B. Inbox | C. Guardian |
|---|---|---|---|
| Idea | Keys are cards you own; approving feels like Apple Pay | One calm list; a request is a message | A security product; one status you read at a glance |
| Look | Light, iOS grouped, provider colors | Warm paper tones, one green | Dark, one lime accent |
| Approve | Bottom sheet, big price, prompt in plain words, Face ID | Swipe right = allow once; tap to read | Full screen, prompt as chat bubbles, hold to approve |
| Shows | A1 home, A2 approve, A3 add key (computer first, paste, get a key) | B1 inbox, B2 scan to pair (camera first, code fallback), B3 connect review (origin, first time ask only) | C1 home, C2 approve, C3 settings (test mode + server under Developer) |
| Icon | 1. Approved key | 2. Key in a bubble | 3. Guard ring (4. MK monogram fits any) |

All directions share the same rules, which Codex can apply now whatever Joseph picks:
1. Price and requester are the largest text on an approval; raw JSON sits behind "Full JSON".
2. Say "apps", "keys", "activity", "allow"; avoid "grant", "policy", "payload" in the UI.
3. Built-in server, nothing to type. Server picker and test mode live under Settings > Developer, off by default, with the explanation from C3.
4. Pairing opens the camera; the MK code and link paste are the "Can't scan?" fallback.
5. Adding a key: "From your computer" first (server page `/add-key`), then paste with a format hint (`sk-proj-...`, `sk-ant-api03-...`), then a link to the provider's key page.
6. No middle dot characters (Joseph's rule).
