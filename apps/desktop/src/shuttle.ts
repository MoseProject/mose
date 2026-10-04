// J/K/L shuttle. L plays forward and each further press doubles the speed (1x, 2x, 4x, 8x), J does
// the same backwards, K stops. The preview engine only plays forward at normal speed, so
// forward 1x is real playback and every other speed walks the playhead with timed seeks, which
// shows the frame under the cursor the way scrubbing does.

import { timelineDuration } from "./editing";
import { api } from "./ipc";
import { createStore, previewStore, run, sessionStore, uiStore } from "./store";

export const SHUTTLE_LEVELS = [1, 2, 4, 8];
const TICK_MS = 80;

export interface ShuttleState {
  /** -1 backwards, 1 forwards, 0 stopped. */
  direction: -1 | 0 | 1;
  /** Speed multiplier, one of {@link SHUTTLE_LEVELS}. */
  level: number;
}

const STOPPED: ShuttleState = { direction: 0, level: 1 };

export const shuttleStore = createStore<ShuttleState>(STOPPED);

/** The state after pressing J (-1) or L (1): a press in a new direction starts at 1x, a repeat
 *  steps up the speeds and stays at the top one. Pure, for testing. */
export function nextShuttle(state: ShuttleState, requested: -1 | 1): ShuttleState {
  if (state.direction !== requested) {
    return { direction: requested, level: SHUTTLE_LEVELS[0] };
  }
  const index = SHUTTLE_LEVELS.indexOf(state.level);
  const next = SHUTTLE_LEVELS[Math.min(SHUTTLE_LEVELS.length - 1, index + 1)];
  return { direction: requested, level: next };
}

let timer: ReturnType<typeof setInterval> | null = null;
let position = 0;

function clearTimer() {
  if (timer !== null) {
    clearInterval(timer);
    timer = null;
  }
}

/** Ends any shuttle without touching playback; the other transport commands call it first. */
export function stopShuttle() {
  clearTimer();
  shuttleStore.set(STOPPED);
}

function tick() {
  const { direction, level } = shuttleStore.get();
  if (direction === 0) {
    clearTimer();
    return;
  }
  const session = sessionStore.get();
  const end = session ? timelineDuration(session) : 0;
  position = Math.min(end, Math.max(0, position + (direction * level * TICK_MS) / 1000));
  void run(api.seek(position));
  if ((direction < 0 && position <= 0) || (direction > 0 && position >= end)) {
    stopShuttle();
  }
}

/** Handles a J (-1) or L (1) press. */
export async function shuttle(requested: -1 | 1) {
  const next = nextShuttle(shuttleStore.get(), requested);
  clearTimer();
  shuttleStore.set(next);
  if (requested === 1 && next.level === 1) {
    await run(api.setPlaying(true));
    return;
  }
  await run(api.setPlaying(false));
  position = previewStore.get().playheadSecs;
  timer = setInterval(tick, TICK_MS);
}

let stopWatching: (() => void) | null = null;

/** Ends a range playback started by {@link playRange}, leaving the transport as it is. */
export function stopRange() {
  stopWatching?.();
  stopWatching = null;
}

/** Plays from the in mark to the out mark and pauses there. A pause by hand or a jump away from
 *  the range ends the watch, so playing later does not stop at the old out mark. */
export async function playRange() {
  const { markIn, markOut } = uiStore.get();
  if (markIn === null || markOut === null || markOut <= markIn) {
    return;
  }
  stopShuttle();
  stopRange();
  await run(api.seek(markIn));
  await run(api.setPlaying(true));
  let seenPlaying = false;
  stopWatching = previewStore.subscribe(() => {
    const tick = previewStore.get();
    if (tick.playing) {
      seenPlaying = true;
    }
    // Ticks that still describe the state before the seek and play requests say nothing yet.
    if (!seenPlaying) {
      return;
    }
    const here = tick.playheadSecs;
    if (!tick.playing || here < markIn - 0.5 || here > markOut + 0.5) {
      stopRange();
    } else if (here >= markOut) {
      stopRange();
      void run(api.setPlaying(false));
    }
  });
}
