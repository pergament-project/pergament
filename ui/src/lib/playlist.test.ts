import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MediaDownload, MediaFile, MediaRef, PlaylistItemInfo, PlaylistItemSource } from "@/lib/api";

const mediaLinks = vi.fn<(media: MediaRef) => Promise<MediaFile[]>>();
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), convertFileSrc: (p: string) => `asset://${p}` }));
vi.mock("@/lib/api", () => ({ api: { mediaLinks: (media: MediaRef) => mediaLinks(media) } }));

import { mediaKeyOf, parseMediaKey, playerItemOf } from "@/lib/playlist";

const VALID = ["pub-sjjm_X_1_AUDIO", "pub-mwbv_X_202609_1_VIDEO", "pub-jwb-143_X_1_VIDEO"];

describe("parseMediaKey", () => {
  it("reads the publication, language, issue, track and kind", () => {
    expect(parseMediaKey("pub-sjjm_X_1_AUDIO")).toEqual({
      pub_symbol: "sjjm",
      issue: null,
      track: 1,
      kind: "audio",
      lang_code: "X",
    });
    expect(parseMediaKey("pub-mwbv_X_202609_1_VIDEO")).toEqual({
      pub_symbol: "mwbv",
      issue: "202609",
      track: 1,
      kind: "video",
      lang_code: "X",
    });
    expect(parseMediaKey("pub-jwb-143_X_1_VIDEO")).toEqual({
      pub_symbol: "jwb-143",
      issue: null,
      track: 1,
      kind: "video",
      lang_code: "X",
    });
  });

  it("refuses keys that are not recordings of a publication", () => {
    for (const key of ["docid-1102016_X_1_VIDEO", "pub-x_X_0_VIDEO", "pub-x_X_1_TEXT", "pub-x_X_2026_1_VIDEO", "pub-a/b_X_1_VIDEO"]) {
      expect(parseMediaKey(key), key).toBeNull();
    }
  });

  it("is the inverse of mediaKeyOf", () => {
    for (const key of VALID) expect(mediaKeyOf(parseMediaKey(key)!)).toBe(key);
  });
});

const item = (source: PlaylistItemSource, extra: Partial<PlaylistItemInfo> = {}): PlaylistItemInfo => ({
  id: 1,
  label: "Song 1",
  kind: source.type === "library" ? source.kind : "video",
  durationTicks: 0,
  startTrimTicks: null,
  endTrimTicks: null,
  endAction: 0,
  thumbnail: null,
  source,
  ...extra,
});
const library: PlaylistItemSource = {
  type: "library",
  keySymbol: "sjjm",
  track: 1,
  issueTag: 0,
  mepsLanguage: 2,
  kind: "audio",
  langCode: "X",
};
const download = {
  id: 1,
  key: "pub-sjjm_X_1_AUDIO",
  langCode: "X",
  title: "Song",
  kind: "audio",
  label: "",
  size: 10,
  duration: null,
  image: null,
  path: "/media/sjjm_1.mp3",
  subtitlePath: null,
  downloadedAt: 0,
} satisfies MediaDownload;

beforeEach(() => {
  mediaLinks.mockReset();
});

describe("playerItemOf", () => {
  it("plays a downloaded copy from disk", async () => {
    const played = await playerItemOf(item(library), [download]);
    expect(played?.sources[0].url).toBe("asset:///media/sjjm_1.mp3");
    expect(mediaLinks).not.toHaveBeenCalled();
  });

  it("streams a recording without a downloaded copy", async () => {
    mediaLinks.mockResolvedValue([{ title: "Song", label: "", url: "https://x/a.mp3", size: 1, subtitles: null, poster: null } as MediaFile]);
    const played = await playerItemOf(item({ ...library, issueTag: 20260900 }), [download]);
    expect(mediaLinks).toHaveBeenCalledWith({ pub_symbol: "sjjm", issue: "202609", track: 1, kind: "audio", lang_code: "X" });
    expect(played?.sources[0].url).toBe("https://x/a.mp3");
    expect(played?.kind).toBe("audio");
  });

  it("keeps all eight digits of a dated issue", async () => {
    mediaLinks.mockResolvedValue([{ title: "Song", label: "", url: "https://x/a.mp3", size: 1, subtitles: null, poster: null } as MediaFile]);
    await playerItemOf(item({ ...library, issueTag: 20260915 }), [download]);
    expect(mediaLinks).toHaveBeenCalledWith({ pub_symbol: "sjjm", issue: "20260915", track: 1, kind: "audio", lang_code: "X" });
  });

  it("passes a failed lookup on to the caller", async () => {
    mediaLinks.mockRejectedValue(new Error("offline"));
    await expect(playerItemOf(item(library), [])).rejects.toThrow("offline");
  });

  it("gives null for a missing file", async () => {
    expect(await playerItemOf(item({ type: "file", path: "/p/a.mp4", mime: "video/mp4", available: false }), [])).toBeNull();
  });

  it("plays a file from disk and applies the trims in seconds", async () => {
    const played = await playerItemOf(
      item({ type: "file", path: "/p/a.mp4", mime: "video/mp4", available: true }, { startTrimTicks: 50_000_000, endTrimTicks: 120_000_000 }),
      [],
    );
    expect(played).toMatchObject({ title: "Song 1", kind: "video", start: 5, end: 12 });
    expect(played?.sources[0].url).toBe("asset:///p/a.mp4");
  });
});
