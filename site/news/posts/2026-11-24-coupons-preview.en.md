---
title: "Coming next: coupons, so you can try an app before adding a key"
date: 2026-11-24
category: news
summary: In the next release, developers will be able to hand out coupons for their own app. You install Keyala, claim one and try the app without connecting an API key. Here is how it is designed.
---

**Design update, November 27:** Full prompt visibility and approval for every call replace the original visibility proposal. The implementation is being validated on staging; production is not enabled. One claim per account per batch; expiry returns unused budget except in-flight or uncertain reservations. Reports are aggregate only. The available budget determines claim capacity, with no separate issued-count limit.
Today, using an app through Keyala means connecting your own API key first. That is a lot to ask from someone who only wants to see whether the app is any good. So we designed **coupons**, and they are planned for the next release.

## The idea in one sentence

A developer pays for a few trial calls in their own app; you claim a coupon in Keyala and try the app without adding an API key.

![Developer pays the provider directly, Keyala keeps the key and the limits, you use one app](../../../media/news/coupon-flow.en.svg)

- **The developer pays the model provider directly.** Usage is billed to their own provider account. Keyala does not sell coupons and never handles the money.
- **Keyala keeps the developer's key and enforces the limits.** The key stays on the server, encrypted, like yours does. You never see it, and you cannot take it to another app.
- **You install Keyala and claim a coupon.** No API key needed. A coupon only works in the app that issued it.

## Limits that are easy to understand

When a developer creates a batch, they choose how much each coupon covers and a total budget, then share the batch link with invitees. The budget is reserved **when you claim**, not when the coupon is sent out. So if someone shares a code but nobody uses it, no budget is wasted.

![30 invitations, US$1 per claim, with a US$20 budget: the first 20 claims each reserve US$1, later claims see a claim-limit message](../../../media/news/coupon-limits.en.svg)

Take the example above: 30 invitations, US$1 per claim, a US$20 budget. The first 20 people to claim each get a full US$1 to use. Person 21 sees "This coupon has reached its claim limit" instead of a trial that stops halfway.

Each claimed coupon also follows a few simple rules:

- **It expires.** By default after 7 days; the developer can choose a different length.
- **The full prompt stays visible.** You review and approve every sponsored call on your phone, including its payer, estimate and remaining allowance. There is no hidden-prompt option.
- **It stops at its limit.** When your coupon or the batch runs out, requests stop with a clear message. Keyala never switches to your own key without asking you.

## What is still open

We are still deciding the finer points: how to handle overspend while several calls are running at once, abuse such as one person claiming many coupons, what happens to unused budget after a coupon expires, and what developers see in their coupon reports. These will be settled before coupons ship, and the developer docs will describe the final behavior.

Coupons are not available yet. If you are joining the TestFlight beta, this is what we are building next, and we would love to hear what you think at [support@keyala.io](mailto:support@keyala.io).
