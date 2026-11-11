// Each episode's YouTube views, and the feed that keeps the newest ones current.
//
// The ai-that-works feed has no view counts, and YouTube's API needs a key, so: a snapshot of every
// video's views, taken from its watch page on 2026-11-11, and the season playlist's public RSS feed
// (no key), which carries live views for its newest 15 videos. A count is the larger of the two, so
// an old video's count only lags, it never drops. The feed also gives the video for an episode the
// ai-that-works feed hasn't linked yet.

const SNAPSHOT: Record<string, number> = {
  _VB9TT1Vus4: 2176,
  '_WLVv1C6-VM': 4608,
  '-doV02eh8XI': 3903,
  '-gFdtc-HbOY': 1092,
  '-N6MajRfqYw': 1495,
  '0dx3j4CmSFw': 2181,
  '0LPBw3NO3Jc': 5063,
  '0rMG-3iiilc': 2261,
  '2ivXNdHJpxk': 1278,
  '4YTl9w_bESE': 543,
  '5Fy0hBzyduU': 708,
  '6B7MzraQMZk': 3517,
  '9MFiATinGC0': 2357,
  '35PSMmDDKP8': 15243,
  '42AzKZRNhsk': 33152,
  '485FGIq8LKM': 3190,
  adpUOpW85ns: 2278,
  b5O6gb_Zuk8: 7573,
  'bak7-C--azc': 984,
  bGMiRmXbRUs: 11195,
  'bLx-UlRTiEw': 522,
  CEGSDlCtI8U: 561,
  'D-pcKduKdYM': 1417,
  fF3GssyaTcc: 3024,
  fOPvAPdqgPo: 7316,
  fyZ0i4USjgc: 6208,
  GF7dnSlCM4U: 24642,
  gkekVC67iVs: 590,
  gRqb7R4Pcrs: 1261,
  grGSFfyejA0: 3911,
  gX9WpYY61xA: 3067,
  HGizPRQfpdw: 14661,
  HsElHU44xJ0: 468,
  h99bTZTR_IU: 2166,
  hTkmSVuDMPg: 1746,
  IkSEXg6f4KY: 4850,
  IXe48aIw_X4: 14264,
  jzhVo0iAX_I: 1168,
  KCqsoXveqiI: 1561,
  KJkvYdGEnAY: 1160,
  LEOA19Ss9lc: 1901,
  l7txtbgCFGU: 1146,
  mAKUL12h1n8: 18486,
  NJcph4j9sNg: 2879,
  'NMhH5_ju3-I': 527,
  niR896pQWOQ: 1258,
  OaUOHEHtlOU: 5868,
  OawyQOrlubM: 584,
  'OpM-G3WNH4g': 4571,
  P5wRLKF4bt8: 1139,
  'PU2h0V-pANQ': 863,
  Ql2gLHWuX7M: 2626,
  qdfwmYTO0Aw: 1933,
  qgAny0sEdIk: 4932,
  RtXpXIY4sLk: 1195,
  RX8D5oJrV9k: 2898,
  'rTn8Vhdt-Jo': 7177,
  sCScFZB4Am8: 625,
  tGbjIvvYuHE: 73331,
  U5Gssat8IUw: 2665,
  UCqD_KUyUJA: 1313,
  VLonoD9m4VY: 19040,
  WHLXRZi8Mus: 8691,
  WhtT7K5Pkv0: 2072,
  wadVIkJnjQE: 1636,
  X5mI1ZVxaIc: 1148,
  'Xece-W7Xf48': 704,
  Xq8VxnGVStg: 4382,
  XUEfIxkdT2Q: 24371,
  'Y-I9m5YsAcs': 833,
  YcT7gjzj2TU: 4099,
  Yh4eL60Ncxs: 13116,
  yxJDyQ8v6P0: 5187,
  Zx_GOhGik0o: 7596,
  z5inaSXkiTU: 2757,
  zNZs19fIDHk: 4902,
  'zpfXzk-3Yxw': 1992,
  zU8GpxgYDvc: 1055,
};

/** The current season's playlist. */
const PLAYLIST = 'PLi60mUelRAbFqfgymVfZttlkIyt0XHZjt';

export type FeedVideo = { id: string; published: string; views: number };

/** The playlist's newest videos with their views; none if YouTube doesn't answer. */
export async function fetchPlaylistVideos(): Promise<FeedVideo[]> {
  try {
    const res = await fetch(
      `https://www.youtube.com/feeds/videos.xml?playlist_id=${PLAYLIST}`,
      { next: { revalidate: 3600 }, signal: AbortSignal.timeout(5000) },
    );
    if (!res.ok) return [];
    const xml = await res.text();
    return [...xml.matchAll(/<entry>([\s\S]*?)<\/entry>/g)].flatMap(([, e]) => {
      const id = e.match(/<yt:videoId>([^<]+)</)?.[1];
      const published = e.match(/<published>([^<]+)</)?.[1];
      const views = Number(e.match(/views="(\d+)"/)?.[1] ?? 0);
      return id && published ? [{ id, published, views }] : [];
    });
  } catch {
    return [];
  }
}

/** A video's views: the snapshot's, or the feed's if it's newer and higher. */
export function viewsFor(id: string, feed: FeedVideo[]): number | undefined {
  const live = feed.find((v) => v.id === id)?.views;
  const snap = SNAPSHOT[id];
  if (live === undefined) return snap;
  return Math.max(live, snap ?? 0);
}

/** A view count the way YouTube writes it: 468, 2.3K, 73K. */
export function formatViews(n: number): string {
  if (n < 1000) return String(n);
  if (n < 10_000) return `${(n / 1000).toFixed(1).replace(/\.0$/, '')}K`;
  if (n < 1_000_000) return `${Math.round(n / 1000)}K`;
  return `${(n / 1_000_000).toFixed(1).replace(/\.0$/, '')}M`;
}
