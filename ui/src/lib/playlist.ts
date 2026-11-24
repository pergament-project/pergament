import { convertFileSrc } from "@tauri-apps/api/core";
import { api, type MediaDownload, type MediaRef, type PlaylistItemInfo } from "@/lib/api";
import type { PlayerItem } from "@/lib/player";
import { localItem, sources } from "@/lib/recording";

/** Ticks of 100 ns in a second, the unit of durations and trims in a playlist. */
export const TICKS_PER_SECOND = 10_000_000;

/** A symbol or language code as `src/links.rs` accepts it. */
const token = (s: string) => s.length > 0 && s.length <= 32 && /^[A-Za-z0-9-]+$/.test(s);

/** `pub-sjjm_X_1_AUDIO`, `pub-mwbv_X_202609_1_VIDEO`, `pub-jwb-143_X_1_VIDEO` → MediaRef; anything else → null. */
export function parseMediaKey(key: string): MediaRef | null {
  if (!key.startsWith("pub-")) return null;
  let body = key.slice("pub-".length);
  let kind: MediaRef["kind"];
  if (body.endsWith("_VIDEO")) kind = "video";
  else if (body.endsWith("_AUDIO")) kind = "audio";
  else return null;
  body = body.slice(0, -"_VIDEO".length);
  const parts = body.split("_");
  if (parts.length !== 3 && parts.length !== 4) return null;
  const [pubSymbol, lang] = parts;
  const issue = parts.length === 4 ? parts[2] : null;
  const track = parts[parts.length - 1];
  if (!token(pubSymbol) || !token(lang)) return null;
  if (issue !== null && !/^(\d{6}|\d{8})$/.test(issue)) return null;
  if (!/^\d+$/.test(track)) return null;
  const n = Number(track);
  if (n <= 0 || n > 0xffff_ffff) return null;
  return { pub_symbol: pubSymbol, issue, track: n, kind, lang_code: lang };
}

/** The naturalKey of a ref, to find a downloaded copy. Inverse of parseMediaKey. */
export function mediaKeyOf(ref: MediaRef): string {
  const issue = ref.issue ? `${ref.issue}_` : "";
  return `pub-${ref.pub_symbol}_${ref.lang_code}_${issue}${ref.track}_${ref.kind.toUpperCase()}`;
}

/** Ticks as seconds; nothing for a missing or zero trim. */
const seconds = (ticks: number | null) => (ticks && ticks > 0 ? ticks / TICKS_PER_SECOND : undefined);

/**
 * A playlist item → PlayerItem: a downloaded copy first, then streamed through api.mediaLinks; files through convertFileSrc.
 * Null when unavailable. Rejects when `api.mediaLinks` fails (no network, for example), so the caller can say why.
 */
export async function playerItemOf(item: PlaylistItemInfo, downloads: MediaDownload[]): Promise<PlayerItem | null> {
  const trims = { start: seconds(item.startTrimTicks), end: seconds(item.endTrimTicks) };
  const thumbnail = item.thumbnail ? convertFileSrc(item.thumbnail) : null;
  const { source } = item;
  if (source.type === "file") {
    if (!source.available) return null;
    return {
      title: item.label,
      kind: item.kind,
      poster: thumbnail,
      sources: [{ label: "", url: convertFileSrc(source.path), size: null, subtitles: null }],
      ...trims,
    };
  }
  if (!source.langCode || source.kind === "image") return null;
  const ref: MediaRef = {
    pub_symbol: source.keySymbol,
    // `IssueTagNumber` is `YYYYMM00` for monthly issues, so the media key has `YYYYMM`;
    // dated issues (`YYYYMMDD`) keep all eight digits.
    issue: source.issueTag ? String(source.issueTag % 100 === 0 ? source.issueTag / 100 : source.issueTag) : null,
    track: source.track,
    kind: source.kind,
    lang_code: source.langCode,
  };
  const key = mediaKeyOf(ref);
  const copy = downloads.find((d) => d.key === key && d.langCode === ref.lang_code);
  if (copy) return { ...localItem(copy), title: item.label, poster: copy.image ?? thumbnail, ...trims };
  const files = await api.mediaLinks(ref);
  if (files.length === 0) return null;
  return { title: item.label, kind: ref.kind, poster: files[0].poster ?? thumbnail, sources: sources(files), ...trims };
}
