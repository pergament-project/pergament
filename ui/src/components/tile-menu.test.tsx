// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { StrictMode } from "react";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

afterEach(cleanup);

const entry = (local: string | null) => ({ item: { symbol: "lff" }, local });
const removePublication = vi.hoisted(() => vi.fn());
const toast = vi.hoisted(() => vi.fn());
const playlistsChanged = vi.hoisted(() => vi.fn());
vi.mock("@/app", () => ({
  useApp: () => ({
    toast,
    favorites: [],
    setFavorite: vi.fn(),
    push: vi.fn(),
    download: vi.fn(),
    removePublication,
    lang: "X",
    userVersion: 0,
    playlistsVersion: 0,
    playlistsChanged,
  }),
}));
const playlistApi = vi.hoisted(() => ({
  playlists: vi.fn(async () => [{ id: 7, name: "Songs", items: 2, durationTicks: 0, thumbnail: null }]),
  createPlaylist: vi.fn(async () => 9),
  addLibraryPlaylistItem: vi.fn(async () => 42),
  setPlaylistItemThumbnail: vi.fn(async () => undefined),
}));
const makeThumbnail = vi.hoisted(() => vi.fn(async () => new Uint8Array([1, 2, 3])));
vi.mock("@/lib/thumbnail", () => ({ makeThumbnail }));
vi.mock("@/lib/api", async (importOriginal) => ({
  ...await importOriginal<typeof import("@/lib/api")>(),
  api: {
    catalogLanguages: vi.fn(async () => [{ code: "X", entry: entry("lff_2") }, { code: "E", entry: entry(null) }]),
    languages: vi.fn(async () => [{ code: "X", name: "German", vernacular: "Deutsch", rtl: false, signLanguage: false }]),
    ...playlistApi,
  },
}));

import { TileMenu } from "@/components/tile-menu";

const target = { kind: "pub", symbol: "lff", keySymbol: "lff", issueTag: 0, mepsLanguage: 2, langCode: "X", local: null, title: "Glücklich" } as const;

// Radix opens a menu on a pointer press, not on a click.
const openMenu = () => fireEvent.pointerDown(screen.getByLabelText("More"), { button: 0, ctrlKey: false });

describe("TileMenu", () => {
  it("opens its menu on click", () => {
    render(<StrictMode><div className="group relative"><TileMenu target={target} /></div></StrictMode>);
    openMenu();
    expect(screen.getByText("Share Link")).toBeTruthy();
    expect(screen.getByText("Add to Favorites")).toBeTruthy();
  });

  it("lists languages", async () => {
    render(<StrictMode><div className="group relative"><TileMenu target={target} /></div></StrictMode>);
    openMenu();
    fireEvent.click(screen.getByText("More Languages"));
    await waitFor(() => expect(screen.getByText("Deutsch")).toBeTruthy());
  });

  it("shares", async () => {
    Object.assign(navigator, { clipboard: { writeText: vi.fn(async () => undefined) } });
    render(<StrictMode><div className="group relative"><TileMenu target={target} /></div></StrictMode>);
    openMenu();
    fireEvent.click(screen.getByText("Share Link"));
    await waitFor(() => expect(navigator.clipboard.writeText).toHaveBeenCalledWith("https://www.jw.org/finder?pub=lff&wtlocale=X"));
  });
});

describe("Remove", () => {
  it("is only offered for a downloaded publication", () => {
    render(<StrictMode><div className="group relative"><TileMenu target={target} /></div></StrictMode>);
    openMenu();
    expect(screen.queryByText("Remove")).toBeNull();
  });

  it("asks first, then deletes", () => {
    removePublication.mockClear();
    render(
      <StrictMode>
        <div className="group relative"><TileMenu target={{ ...target, local: "lff_2" }} /></div>
      </StrictMode>,
    );
    openMenu();
    fireEvent.click(screen.getByText("Remove"));
    expect(removePublication).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Remove" }));
    expect(removePublication).toHaveBeenCalledWith("lff_2", "Glücklich");
  });

  it("does nothing when cancelled", () => {
    removePublication.mockClear();
    render(
      <StrictMode>
        <div className="group relative"><TileMenu target={{ ...target, local: "lff_2" }} /></div>
      </StrictMode>,
    );
    openMenu();
    fireEvent.click(screen.getByText("Remove"));
    fireEvent.click(screen.getByText("Cancel"));
    expect(removePublication).not.toHaveBeenCalled();
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });
});

describe("Add to Playlist", () => {
  const media = {
    kind: "media",
    key: "pub-sjjm_X_12_AUDIO",
    langCode: "X",
    title: "Jehovas Eigenschaften",
    duration: 185.5,
    image: "https://example.org/sjjm.jpg",
  } as const;
  const ref = { pub_symbol: "sjjm", issue: null, track: 12, kind: "audio", lang_code: "X" };
  const renderMenu = (t: Parameters<typeof TileMenu>[0]["target"] = media) =>
    render(<StrictMode><div className="group relative"><TileMenu target={t} /></div></StrictMode>);

  const reset = () => {
    toast.mockClear();
    playlistsChanged.mockClear();
    Object.values(playlistApi).forEach((f) => f.mockClear());
    makeThumbnail.mockClear();
  };

  it("is hidden for a recording that is not from a publication", () => {
    renderMenu({ ...media, key: "docid-502013162_1_VIDEO" });
    openMenu();
    expect(screen.getByText("Share Link")).toBeTruthy();
    expect(screen.queryByText("Add to Playlist")).toBeNull();
  });

  it("is shown for a publication recording", () => {
    renderMenu();
    openMenu();
    expect(screen.getByText("Add to Playlist")).toBeTruthy();
  });

  it("adds the recording to the picked playlist", async () => {
    reset();
    renderMenu();
    openMenu();
    fireEvent.click(screen.getByText("Add to Playlist"));
    fireEvent.click(await screen.findByText("Songs"));
    await waitFor(() => expect(toast).toHaveBeenCalledWith("Added to Songs"));
    expect(playlistApi.addLibraryPlaylistItem).toHaveBeenCalledWith(7, ref, "Jehovas Eigenschaften", 185.5);
    expect(playlistsChanged).toHaveBeenCalled();
    // The thumbnail follows in the background.
    await waitFor(() => expect(playlistApi.setPlaylistItemThumbnail).toHaveBeenCalledWith(42, new Uint8Array([1, 2, 3])));
    expect(makeThumbnail).toHaveBeenCalledWith("https://example.org/sjjm.jpg", "image");
    expect(screen.queryByText("Songs")).toBeNull();
  });

  it("ignores a failed thumbnail", async () => {
    reset();
    makeThumbnail.mockResolvedValueOnce(null as never);
    renderMenu();
    openMenu();
    fireEvent.click(screen.getByText("Add to Playlist"));
    fireEvent.click(await screen.findByText("Songs"));
    await waitFor(() => expect(toast).toHaveBeenCalledWith("Added to Songs"));
    await waitFor(() => expect(makeThumbnail).toHaveBeenCalled());
    expect(playlistApi.setPlaylistItemThumbnail).not.toHaveBeenCalled();
    expect(toast).toHaveBeenCalledTimes(1);
  });

  it("shows an error when adding fails", async () => {
    reset();
    playlistApi.addLibraryPlaylistItem.mockRejectedValueOnce("boom" as never);
    renderMenu();
    openMenu();
    fireEvent.click(screen.getByText("Add to Playlist"));
    fireEvent.click(await screen.findByText("Songs"));
    await waitFor(() => expect(toast).toHaveBeenCalledWith("boom"));
    expect(makeThumbnail).not.toHaveBeenCalled();
  });

  it("creates a playlist, then adds the recording to it", async () => {
    reset();
    renderMenu();
    openMenu();
    fireEvent.click(screen.getByText("Add to Playlist"));
    fireEvent.click(await screen.findByRole("button", { name: "Create Playlist" }));
    fireEvent.change(screen.getByPlaceholderText("New playlist"), { target: { value: "Morning" } });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    await waitFor(() => expect(toast).toHaveBeenCalledWith("Added to Morning"));
    expect(playlistApi.createPlaylist).toHaveBeenCalledWith("Morning");
    expect(playlistApi.addLibraryPlaylistItem).toHaveBeenCalledWith(9, ref, "Jehovas Eigenschaften", 185.5);
    expect(playlistApi.createPlaylist.mock.invocationCallOrder[0]).toBeLessThan(
      playlistApi.addLibraryPlaylistItem.mock.invocationCallOrder[0],
    );
  });
});
