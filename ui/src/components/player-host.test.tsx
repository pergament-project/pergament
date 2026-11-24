// @vitest-environment jsdom
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { PlayerItem } from "@/lib/player";

/** The parts of the app context the player uses, kept in a small host so the queue can change. */
type Ctx = {
  player: { id: number; items: PlayerItem[]; index: number } | null;
  playItem: (item: PlayerItem) => void;
  playQueue: (items: PlayerItem[], index: number) => void;
  closePlayer: () => void;
};
let ctx: Ctx;
const send = vi.fn();
vi.mock("@/app", () => ({
  useApp: () => ({ ...ctx, lang: "X", view: { name: "home" }, display: { enabled: false, send }, viewer: null }),
}));
vi.mock("@/lib/api", () => ({ api: { openExternal: vi.fn() } }));

import { PlayerHost } from "@/components/player-host";

function Host() {
  const [player, setPlayer] = useState<Ctx["player"]>(null);
  const playQueue = (items: PlayerItem[], index: number) => setPlayer({ id: Math.random(), items, index });
  ctx = { player, playQueue, playItem: (item) => playQueue([item], 0), closePlayer: () => setPlayer(null) };
  return <PlayerHost />;
}

afterEach(cleanup);

const one = (title: string, kind: PlayerItem["kind"], url: string): PlayerItem => ({
  title,
  kind,
  poster: null,
  sources: [{ label: "", url, size: null, subtitles: null }],
});
const audio = one("Song", "audio", "asset://song.mp3");
const image = one("Photo", "image", "asset://photo.png");
const video = one("Film", "video", "asset://film.mp4");
const video_ = () => document.querySelector("video");

describe("Player queue", () => {
  it("moves on when an item ends, waits on a picture for Next, and plays a single item without the queue buttons", () => {
    render(<Host />);
    act(() => ctx.playQueue([audio, image, video], 0));
    expect(video_()?.getAttribute("src")).toBe("asset://song.mp3");
    expect(screen.getByLabelText("Previous")).toBeTruthy();

    fireEvent.ended(video_()!);
    expect(video_()).toBeNull();
    expect(screen.getByAltText("Photo").getAttribute("src")).toBe("asset://photo.png");

    fireEvent.click(screen.getByLabelText("Next"));
    expect(video_()?.getAttribute("src")).toBe("asset://film.mp4");
    expect((screen.getByLabelText("Next") as HTMLButtonElement).disabled).toBe(true);

    fireEvent.click(screen.getAllByLabelText("Close")[0]);
    expect(video_()).toBeNull();
    act(() => ctx.playItem(video));
    expect(video_()?.getAttribute("src")).toBe("asset://film.mp4");
    expect(screen.queryByLabelText("Previous")).toBeNull();
    expect(screen.queryByLabelText("Next")).toBeNull();
  });

  it("says so when a picture cannot be shown", () => {
    render(<Host />);
    act(() => ctx.playQueue([image, video], 0));
    fireEvent.error(screen.getByAltText("Photo"));
    expect(screen.getByRole("alert").textContent).toBe("This picture cannot be shown.");
  });

  it("starts at the start trim and goes on at the end trim", () => {
    render(<Host />);
    act(() => ctx.playQueue([{ ...audio, start: 5, end: 12 }, video], 0));
    const el = video_()!;
    fireEvent.loadedMetadata(el);
    expect(el.currentTime).toBe(5);
    Object.defineProperty(el, "paused", { value: false });
    el.currentTime = 12;
    fireEvent.timeUpdate(el);
    expect(video_()?.getAttribute("src")).toBe("asset://film.mp4");
  });
});
