// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";

const calls: { cmd: string; args: unknown }[] = [];
let playlists: { id: number; name: string; items: number; durationTicks: number; thumbnail: string | null }[] = [];
let playlistItems: {
  id: number;
  label: string;
  kind: "audio" | "video" | "image";
  durationTicks: number;
  startTrimTicks: number | null;
  endTrimTicks: number | null;
  endAction: number;
  thumbnail: string | null;
  source:
    | { type: "library"; keySymbol: string; track: number; issueTag: number; mepsLanguage: number; kind: "audio" | "video" | "image"; langCode: string | null }
    | { type: "file"; path: string; mime: string; available: boolean };
}[] = [];

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string, args?: unknown) => {
    calls.push({ cmd, args });
    if (cmd === "playlists") return playlists;
    if (cmd === "playlist") {
      const id = (args as { id: number }).id;
      return { id, name: playlists.find((p) => p.id === id)?.name ?? "", items: playlistItems };
    }
    if (cmd === "create_playlist") return 1;
    if (cmd === "rename_playlist") return undefined;
    if (cmd === "delete_playlist") return undefined;
    if (cmd === "remove_playlist_item") return undefined;
    if (cmd === "move_playlist_item") return undefined;
    if (cmd === "import_playlist") return 2;
    if (cmd === "export_playlist") return undefined;
    return undefined;
  }),
  convertFileSrc: (p: string) => p,
}));

const push = vi.fn();
const toast = vi.fn();
const playQueue = vi.fn();
const showImage = vi.fn();
let userVersion = 0;
let notesVersion = 0;
let playlistsVersion = 0;
const playlistsChanged = vi.fn();
let currentView: { name: string; id?: number } = { name: "personal" };

vi.mock("@/app", () => ({
  useApp: () => ({
    publications: [],
    lang: "E",
    bibles: {},
    openTarget: vi.fn(),
    push,
    toast,
    userVersion,
    notesVersion,
    playlistsVersion,
    playlistsChanged,
    playQueue,
    showImage,
    setFavorite: vi.fn(),
    removePublication: vi.fn(),
    createBackup: vi.fn(),
    restoreBackup: vi.fn(),
    mergeBackups: false,
    openNote: vi.fn(),
    note: null,
    releaseNote: vi.fn(),
    keepNote: vi.fn(),
    view: currentView,
    replace: vi.fn(),
    favorites: [],
    catalogVersion: 0,
    loadCatalog: vi.fn(),
    download: vi.fn(),
    updateOne: vi.fn(),
    updateAll: vi.fn(),
    update: null,
    display: { enabled: false, setEnabled: vi.fn(), send: vi.fn() },
    popOutNote: vi.fn(),
  }),
  SectionTitle: ({ children, aside }: { children: React.ReactNode; aside?: React.ReactNode }) => (
    <div>
      <h2>{children}</h2>
      {aside}
    </div>
  ),
  AppBar: ({ title, children }: { title: string; children?: React.ReactNode }) => (
    <header>
      <h1>{title}</h1>
      {children}
    </header>
  ),
  BarButton: ({ label, children, onClick, disabled }: { label: string; children: React.ReactNode; onClick: () => void; disabled?: boolean }) => (
    <button onClick={onClick} disabled={disabled} aria-label={label}>
      {children}
    </button>
  ),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
  save: vi.fn(),
  ask: vi.fn(),
}));

vi.mock("@/components/media-card", () => ({
  useMediaDownloads: () => ({ list: [] }),
}));

vi.mock("@/lib/playlist", () => ({
  TICKS_PER_SECOND: 10_000_000,
  playerItemOf: vi.fn(async (item) => {
    const src = item.source.type === "file" ? item.source.path : `library://${item.source.keySymbol}`;
    return {
      title: item.label,
      kind: item.kind,
      poster: null,
      sources: [{ label: "", url: src, size: null, subtitles: null }],
    };
  }),
}));

import { PlaylistsSection } from "./playlists";
import { PlaylistView } from "@/views/playlist";

const sent = (cmd: string) => calls.filter((c) => c.cmd === cmd).map((c) => c.args);

beforeEach(async () => {
  calls.length = 0;
  playlists = [
    { id: 1, name: "Songs", items: 3, durationTicks: 300_000_000, thumbnail: null },
    { id: 2, name: "Videos", items: 2, durationTicks: 200_000_000, thumbnail: null },
  ];
  playlistItems = [
    {
      id: 10,
      label: "Song 1",
      kind: "audio",
      durationTicks: 100_000_000,
      startTrimTicks: null,
      endTrimTicks: null,
      endAction: 0,
      thumbnail: null,
      source: { type: "library", keySymbol: "sjjm", track: 1, issueTag: 0, mepsLanguage: 2, kind: "audio", langCode: "de" },
    },
    {
      id: 11,
      label: "Video 1",
      kind: "video",
      durationTicks: 100_000_000,
      startTrimTicks: null,
      endTrimTicks: null,
      endAction: 0,
      thumbnail: null,
      source: { type: "file", path: "test.mp4", mime: "video/mp4", available: true },
    },
    {
      id: 12,
      label: "Photo 1",
      kind: "image",
      durationTicks: 40_000_000,
      startTrimTicks: null,
      endTrimTicks: null,
      endAction: 0,
      thumbnail: null,
      source: { type: "file", path: "photo.jpg", mime: "image/jpeg", available: false },
    },
  ];
  userVersion = 0;
  notesVersion = 0;
  playlistsVersion = 0;
  currentView = { name: "personal" };
  playlistsChanged.mockClear();
  push.mockClear();
  toast.mockClear();
  playQueue.mockClear();
  showImage.mockClear();
  const { playerItemOf } = await import("@/lib/playlist");
  vi.mocked(playerItemOf).mockImplementation(async (item) => {
    const src = item.source.type === "file" ? item.source.path : `library://${item.source.keySymbol}`;
    return {
      title: item.label,
      kind: item.kind,
      poster: null,
      sources: [{ label: "", url: src, size: null, subtitles: null }],
    };
  });
});
afterEach(cleanup);

async function openCreateDialog() {
  const btn = await screen.findByRole("button", { name: /Add/i });
  fireEvent.pointerDown(btn, { button: 0, ctrlKey: false });
  await screen.findByRole("menuitem", { name: /Create Playlist/i });
  fireEvent.click(screen.getByRole("menuitem", { name: /Create Playlist/i }));
  return screen.findByRole("dialog");
}

async function openImportDialog() {
  const btn = await screen.findByRole("button", { name: /Add/i });
  fireEvent.pointerDown(btn, { button: 0, ctrlKey: false });
  await screen.findByRole("menuitem", { name: /Import Playlist/i });
  fireEvent.click(screen.getByRole("menuitem", { name: /Import Playlist/i }));
}

describe("PlaylistsSection", () => {
  it("lists playlists from playlists", async () => {
    render(<PlaylistsSection />);
    expect(await screen.findByText("Songs")).toBeTruthy();
    expect(screen.getByText("3 items")).toBeTruthy();
    expect(screen.getByText("Videos")).toBeTruthy();
    expect(screen.getByText("2 items")).toBeTruthy();
  });

  it("create playlist calls create_playlist and reloads", async () => {
    render(<PlaylistsSection />);
    const dialog = await openCreateDialog();
    const input = within(dialog).getByLabelText(/Name/i);
    fireEvent.change(input, { target: { value: "New Playlist" } });
    fireEvent.click(within(dialog).getByRole("button", { name: /Create/i }));
    await waitFor(() => expect(sent("create_playlist")).toEqual([{ name: "New Playlist" }]));
    await waitFor(() => expect(sent("playlists").length).toBeGreaterThan(0));
  });

  it("create playlist rejects empty or duplicate name", async () => {
    render(<PlaylistsSection />);
    const dialog = await openCreateDialog();
    const input = within(dialog).getByLabelText(/Name/i);
    const submitBtn = within(dialog).getByRole("button", { name: /Create/i });

    // Empty name
    fireEvent.change(input, { target: { value: "   " } });
    fireEvent.click(submitBtn);
    expect(await within(dialog).findByText(/Enter a name/i)).toBeTruthy();
    expect(sent("create_playlist")).toHaveLength(0);

    // Duplicate name (case-insensitive)
    fireEvent.change(input, { target: { value: "songs" } });
    fireEvent.click(submitBtn);
    expect(await within(dialog).findByText(/already exists/i)).toBeTruthy();
    expect(sent("create_playlist")).toHaveLength(0);
  });

  it("delete asks first: cancel calls nothing, confirm calls delete_playlist", async () => {
    render(<PlaylistsSection />);
    const moreButtons = await screen.findAllByRole("button", { name: /More|⋯|three dots/i });
    fireEvent.pointerDown(moreButtons[0], { button: 0, ctrlKey: false });
    await screen.findByRole("menuitem", { name: /Delete/i });
    fireEvent.click(screen.getByRole("menuitem", { name: /Delete/i }));
    const alert = await screen.findByRole("alertdialog");
    fireEvent.click(within(alert).getByRole("button", { name: /Cancel/i }));
    expect(sent("delete_playlist")).toHaveLength(0);

    const moreButtonsAgain = screen.getAllByRole("button", { name: /More|⋯|three dots/i });
    fireEvent.pointerDown(moreButtonsAgain[0], { button: 0, ctrlKey: false });
    await screen.findByRole("menuitem", { name: /Delete/i });
    fireEvent.click(screen.getByRole("menuitem", { name: /Delete/i }));
    const alert2 = await screen.findByRole("alertdialog");
    fireEvent.click(within(alert2).getByRole("button", { name: /Delete/i }));
    await waitFor(() => expect(sent("delete_playlist")).toEqual([{ id: 1 }]));
  });

  it("import: dialog returns path → import_playlist → push playlist view", async () => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    vi.mocked(open).mockResolvedValue("/tmp/test.jwlplaylist");
    render(<PlaylistsSection />);
    await openImportDialog();
    await waitFor(() => expect(sent("import_playlist")).toEqual([{ path: "/tmp/test.jwlplaylist" }]));
    await waitFor(() =>
      expect(push).toHaveBeenCalledWith(expect.objectContaining({ name: "playlist", id: 2 })),
    );
    expect(playlistsChanged).toHaveBeenCalled();
  });

  it("shows 'No playlists yet' when empty", async () => {
    playlists = [];
    render(<PlaylistsSection />);
    expect(await screen.findByText(/No playlists yet/i)).toBeTruthy();
  });
});

describe("PlaylistView", () => {
  beforeEach(() => {
    currentView = { name: "playlist", id: 1 };
  });

  it("renders items in order with Unavailable badge", async () => {
    render(<PlaylistView />);
    expect(await screen.findByText("Song 1")).toBeTruthy();
    expect(screen.getByText("Video 1")).toBeTruthy();
    expect(screen.getByText("Photo 1")).toBeTruthy();
    expect(screen.getByText(/Unavailable/i)).toBeTruthy();
  });

  it("clicking item 2 calls playQueue with index 1 among available items", async () => {
    render(<PlaylistView />);
    expect(await screen.findByText("Video 1")).toBeTruthy();
    const rows = screen.getAllByRole("row");
    // Item 1 (0-based) is "Video 1" (available), index 1 among available (Song 1, Video 1)
    fireEvent.click(rows[1]);
    await waitFor(() =>
      expect(playQueue).toHaveBeenCalledWith(
        expect.arrayContaining([
          expect.objectContaining({ title: "Song 1" }),
          expect.objectContaining({ title: "Video 1" }),
        ]),
        1,
      ),
    );
  });

  it("clamps playQueue index when earlier items fail to load", async () => {
    const { playerItemOf } = await import("@/lib/playlist");
    // Make Song 1 fail, so only Video 1 loads
    vi.mocked(playerItemOf).mockImplementation(async (item) => {
      if (item.id === 10) return null;
      return {
        title: item.label,
        kind: item.kind,
        poster: null,
        sources: [{ label: "", url: "test", size: null, subtitles: null }],
      };
    });
    render(<PlaylistView />);
    expect(await screen.findByText("Video 1")).toBeTruthy();
    const rows = screen.getAllByRole("row");
    // Click Video 1 (index 1 among available). Song 1 dropped -> targetIndex adjusted and clamped to 0
    fireEvent.click(rows[1]);
    await waitFor(() =>
      expect(playQueue).toHaveBeenCalledWith(
        [expect.objectContaining({ title: "Video 1" })],
        0,
      ),
    );
    expect(toast).toHaveBeenCalledWith(expect.stringContaining("could not be loaded"));
  });

  it("remove calls remove_playlist_item", async () => {
    render(<PlaylistView />);
    expect(await screen.findByText("Song 1")).toBeTruthy();
    const moreButtons = screen.getAllByRole("button", { name: /More|⋯|three dots/i });
    fireEvent.pointerDown(moreButtons[0], { button: 0, ctrlKey: false });
    await screen.findByRole("menuitem", { name: /Remove/i });
    fireEvent.click(screen.getByRole("menuitem", { name: /Remove/i }));
    await waitFor(() => expect(sent("remove_playlist_item")).toEqual([{ item: 10 }]));
  });

  it("reorder calls move_playlist_item with the right 0-based position", async () => {
    render(<PlaylistView />);
    expect(await screen.findByText("Song 1")).toBeTruthy();
    const moreButtons = screen.getAllByRole("button", { name: /More|⋯|three dots/i });
    fireEvent.pointerDown(moreButtons[0], { button: 0, ctrlKey: false });
    await screen.findByRole("menuitem", { name: /Move down/i });
    fireEvent.click(screen.getByRole("menuitem", { name: /Move down/i }));
    await waitFor(() => expect(sent("move_playlist_item")).toEqual([{ playlist: 1, item: 10, position: 1 }]));
  });

  it("clicking photo calls showImage when available", async () => {
    // Add an available photo item
    playlistItems.push({
      id: 13,
      label: "Available Photo",
      kind: "image",
      durationTicks: 40_000_000,
      startTrimTicks: null,
      endTrimTicks: null,
      endAction: 0,
      thumbnail: null,
      source: { type: "file", path: "/path/to/photo.png", mime: "image/png", available: true },
    });
    render(<PlaylistView />);
    expect(await screen.findByText("Available Photo")).toBeTruthy();
    const rows = screen.getAllByRole("row");
    // Row 3 is "Available Photo"
    fireEvent.click(rows[3]);
    await waitFor(() =>
      expect(showImage).toHaveBeenCalledWith("/path/to/photo.png", "Available Photo"),
    );
  });

  it("play all button queues available items", async () => {
    render(<PlaylistView />);
    expect(await screen.findByText("Song 1")).toBeTruthy();
    const playAllBtn = screen.getByRole("button", { name: /Play all/i });
    fireEvent.click(playAllBtn);
    await waitFor(() =>
      expect(playQueue).toHaveBeenCalledWith(
        expect.arrayContaining([
          expect.objectContaining({ title: "Song 1" }),
          expect.objectContaining({ title: "Video 1" }),
        ]),
        0,
      ),
    );
  });

  it("export playlist triggers export_playlist invoke", async () => {
    const { save } = await import("@tauri-apps/plugin-dialog");
    vi.mocked(save).mockResolvedValue("/tmp/exported.jwlplaylist");
    render(<PlaylistView />);
    expect(await screen.findByText("Song 1")).toBeTruthy();
    const exportBtn = screen.getByRole("button", { name: /Export/i });
    fireEvent.click(exportBtn);
    await waitFor(() =>
      expect(sent("export_playlist")).toEqual([
        { id: 1, path: "/tmp/exported.jwlplaylist" },
      ]),
    );
  });
});
