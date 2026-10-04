# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users
Video editors cutting long gameplay recordings (hours of footage per session) into YouTube videos, starting with the PacoPaçoca channel. They work at a desktop, mostly on the keyboard, and spend their time scrubbing, cutting and rearranging on a timeline.

## Product Purpose
oca is a native desktop video editor (Tauri shell over a Rust engine) for cutting gameplay footage. Success is a finished cut exported with consistent loudness and without losing quality, with as little friction as possible between raw recording and upload.

## Positioning
Automatic loudness normalization and export that matches the source bitrate, in a native, lightweight app. A generic editor makes the user chase both by hand.

## Operating Context
This is a desktop application, not a website: a Tauri window whose interface happens to be rendered by a webview. Design for a resizable desktop window (mouse, keyboard, large monitors), not for phones or touch; no mobile breakpoints, no page scrolling, no browser chrome. Native behaviour (file dialogs, OS drag-and-drop, window resizing) comes from the shell. `web` under Platform only records that the UI is HTML/CSS.

Long sessions on a desktop monitor; large source files; an editing workspace (library, program monitor, inspector, timeline) that must stay readable for hours. Heavy work (export, transcription, analysis) runs in the background and reports progress.

## Capabilities and Constraints
Multi-track timeline with video, audio, text and shape tracks; keyframes; transcription, scene and highlight detection; export queue with loudness matching. The UI ships in pt-BR and English and every display string lives in `apps/desktop/src/i18n.ts`. Keyboard shortcuts are part of the workflow. The webview talks to Rust only through `apps/desktop/src/ipc.ts`.

## Brand Commitments
The product is named oca. The top bar shows the logo (`logo.png`) without a wordmark.

## Evidence on Hand
Reference mockup for the editor layout supplied by the user (left tool rail, top bar with timecode/fps/resolution readout, media grid, program monitor with transport and scrubber, sectioned inspector, multi-track timeline with thumbnails and waveforms). No customer or usage data.

## Product Principles
- The editing workspace outranks application chrome.
- Direct manipulation and keyboard efficiency over modal flows.
- Progressive disclosure: contextual tools appear for the selected object.
- Background work reports progress without covering the work.

## Accessibility & Inclusion
pt-BR and English; keyboard-first operation. No formal WCAG requirement beyond readable contrast and visible focus.
