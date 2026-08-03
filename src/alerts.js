//! When to beep.
//
// Pure state, no audio and no DOM — the decision is testable in Node, while
// making the sound is the webview's job. Per DESIGN.md, readings arrive late
// and bunched, so the three rules encoded here are all consequences of that:
// a burst sounds once, startup is silent, and each threshold fires at most
// once per window.

/// Percent *used*, not remaining. Deliberately not the colour band points —
/// see DESIGN.md for why the two signals are kept apart.
export const THRESHOLDS = [50, 80, 90, 95];

/// The state after seeing `reading` for the first time, with nothing to
/// announce. Every threshold already passed counts as announced: reopening
/// the window at 91% used must not beep, or the beep becomes noise the user
/// learns to ignore.
export function initialAlertState(reading) {
  return {
    resetsAt: reading?.resets_at ?? null,
    fired: THRESHOLDS.filter((t) => crossed(reading, t)),
  };
}

/// Folds a new reading into the state, returning the state to keep and the
/// threshold to sound, if any.
///
/// `beepAt` is the *highest* threshold crossed by this update, never a list:
/// one update that jumps 48% → 92% is one piece of news, and four beeps in a
/// row convey nothing the single beep does not.
export function nextAlertState(state, reading) {
  // No usable reading — hold the state and stay quiet. A missing reading is
  // not a reset, and treating it as one would re-arm every threshold.
  if (!reading || typeof reading.used_percentage !== "number") {
    return { state, beepAt: null };
  }

  // A new window: usage only falls at a reset, so this is the one thing that
  // legitimately re-arms a threshold. Crossings in the new window then sound
  // normally — including on its first reading, which is real news rather than
  // the startup case handled by initialAlertState.
  const fired = reading.resets_at === state.resetsAt ? state.fired : [];

  const newlyCrossed = THRESHOLDS.filter((t) => crossed(reading, t) && !fired.includes(t));

  return {
    // Every threshold crossed is marked fired, not just the one that sounded,
    // so the ones passed silently during a burst stay silent afterwards.
    state: { resetsAt: reading.resets_at, fired: [...fired, ...newlyCrossed] },
    beepAt: newlyCrossed.length ? Math.max(...newlyCrossed) : null,
  };
}

function crossed(reading, threshold) {
  return typeof reading?.used_percentage === "number" && reading.used_percentage >= threshold;
}
