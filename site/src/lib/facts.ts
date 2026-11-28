// Every figure the page quotes, with where it comes from: docs/benchmarks.md,
// the 1.0.0 series. Change them here when that page changes.

/** The conditions under every figure below, as the page states them. */
export const SETUP =
  'A 14″ MacBook Pro (M4 Pro) as the client, a Windows 11 PC with an RTX 3070 Ti as the host, on Ethernet; every stack at the same bitrate on the wire; timed while the client’s screen is recorded, the same way for each.';

export interface Versus {
  helio: number;
  apollo: number;
  sunshine: number;
}

/**
 * "Click to frame, recorded", series F: a click in a game (the scene drawn
 * every refresh) to the frame that shows it, median of five runs of 58
 * clicks, in ms. Helio matches Apollo at 1080p60 and is faster at 1440p120;
 * the other four modes are on that page.
 */
export const CLICK: (Versus & { mode: string })[] = [
  { mode: '1080p at 60 fps', helio: 54.1, apollo: 55.0, sunshine: 59.0 },
  { mode: '1440p at 120 fps', helio: 51.5, apollo: 55.8, sunshine: 55.0 },
];

/**
 * "Starting a stream", in s: Helio's median of its three starts with no
 * virtual display kept from an earlier session (1.02–1.23 s; a client back
 * within a minute takes 0.42 s), Moonlight's medians with the host's monitor
 * awake, whose starts are no faster after an earlier one. 2560x1440 at 120
 * fps, on the release candidate before the two 60 fps fixes, which do not
 * change the host at that rate.
 */
export const START: Versus = { helio: 1.1, apollo: 2.8, sunshine: 4.3 };

/** "Frames shown", 2560x1440 at 120 fps, the counter scene: % of the host's frames. */
export const FRAMES: Versus = { helio: 97.3, apollo: 86.4, sunshine: 83.4 };

/** "Bad links", L1, 5% of packets lost both ways, 2560x1440 at 120 fps: % of the host's frames. */
export const LOSS: Versus = { helio: 96.1, apollo: 84.3, sunshine: 85.2 };

/**
 * "Bad links", L4n: 60 Mbit/s, then 20 for 20 s, then 60, at 2560x1440 at
 * 60 fps on noise; % of the host's frames shown while it narrows. Helio's
 * is a floor: at 60 fps the recording misses frames it shows.
 */
export const NARROWING: Versus = { helio: 84.8, apollo: 40.5, sunshine: 20.7 };

/** "Long sessions": an hour each at 2560x1440 at 120 fps, % of frames shown, mean of the minutes. */
export const HOUR: Versus = { helio: 96.5, apollo: 85.2, sunshine: 85.0 };

/** "Long sessions": the client's memory in the hour's last minute, MB (Helio's was 103 in the first too). */
export const MEMORY: Versus = { helio: 103, apollo: 225, sunshine: 224 };

/** "Load", 2560x1440 at 120 fps on noise: the client's CPU, % of a core, median of five runs. */
export const CLIENT_CPU: Versus = { helio: 26.7, apollo: 43.5, sunshine: 43.8 };

/**
 * "Starting a stream", in s: the medians of the later starts with the
 * monitor awake, 10 s after the last session. Helio Host on Windows keeps
 * the virtual display for a minute after a session (LINGER,
 * helio-host/src/platform.rs; the macOS and Linux hosts make a new one each
 * time), so a client back in that minute skips making it; Moonlight's
 * starts take as long as its first.
 */
export const RETURN: Versus = { helio: 0.42, apollo: 2.82, sunshine: 4.26 };

/** docs/benchmarks.md, pq-boringtun on an Apple M4 Pro (Criterion). */
export const TUNNEL = {
  encrypt1200: '0.63 µs',
};
