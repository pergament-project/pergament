import { useCallback, useEffect, useState } from "react";
import { ListMusic, MoreHorizontal, Pencil, Play, Share2, Trash2 } from "lucide-react";
import { AppBar, BarButton, useApp } from "@/app";
import { api, type Playlist, type PlaylistItemInfo } from "@/lib/api";
import { convertFileSrc } from "@tauri-apps/api/core";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { playerItemOf, TICKS_PER_SECOND } from "@/lib/playlist";
import { formatDuration, type PlayerItem } from "@/lib/player";
import { useMediaDownloads } from "@/components/media-card";
import { DeleteConfirm, NameDialog } from "@/components/playlists";
import { t } from "@/lib/i18n";

function isItemAvailable(item: PlaylistItemInfo): boolean {
  if (item.source.type === "file") return item.source.available;
  return item.source.langCode !== null;
}

export function PlaylistView() {
  const { view, push, toast, playlistsVersion, playlistsChanged, playQueue, showImage } = useApp();
  const playlistId = view.name === "playlist" ? view.id : 0;
  const { list: downloads } = useMediaDownloads();

  const [playlist, setPlaylist] = useState<Playlist | null>(null);
  const [renaming, setRenaming] = useState(false);
  const [existingNames, setExistingNames] = useState<string[]>([]);
  const [deleting, setDeleting] = useState(false);

  const load = useCallback(async () => {
    if (!playlistId) return;
    try {
      const data = await api.playlist(playlistId);
      setPlaylist(data);
    } catch (e) {
      toast(String(e));
    }
  }, [playlistId, toast]);

  useEffect(() => {
    void load();
  }, [load, playlistsVersion]);

  const availableItems = playlist?.items.filter(isItemAvailable) ?? [];

  const handlePlayAll = async () => {
    if (!playlist || availableItems.length === 0) return;
    const results = await Promise.allSettled(
      availableItems.map((item) => playerItemOf(item, downloads)),
    );
    const loadedItems: PlayerItem[] = [];
    let dropped = 0;
    results.forEach((r) => {
      if (r.status === "fulfilled" && r.value) {
        loadedItems.push(r.value);
      } else {
        dropped++;
      }
    });
    if (dropped > 0) {
      toast(t("Some items could not be loaded and were left out"));
    }
    if (loadedItems.length > 0) {
      playQueue(loadedItems, 0);
    }
  };

  const handleItemClick = async (clickedItem: PlaylistItemInfo) => {
    if (!isItemAvailable(clickedItem)) return;

    if (clickedItem.kind === "image") {
      if (clickedItem.source.type === "file") {
        showImage(convertFileSrc(clickedItem.source.path), clickedItem.label);
      }
      // Library items in JW Library playlists only ever have audio/video (MajorMultimediaType 0, 2, 3), so file items are the only image source.
      return;
    }

    const availableIndex = availableItems.findIndex((i) => i.id === clickedItem.id);
    if (availableIndex === -1) return;

    const results = await Promise.allSettled(
      availableItems.map((item) => playerItemOf(item, downloads)),
    );
    const loadedItems: PlayerItem[] = [];
    let targetIndex = availableIndex;
    let dropped = 0;
    results.forEach((r, idx) => {
      if (r.status === "fulfilled" && r.value) {
        loadedItems.push(r.value);
      } else {
        dropped++;
        if (idx < availableIndex) {
          targetIndex--;
        }
      }
    });
    if (dropped > 0) {
      toast(t("Some items could not be loaded and were left out"));
    }
    if (loadedItems.length > 0) {
      const safeIndex = Math.min(Math.max(0, targetIndex), loadedItems.length - 1);
      playQueue(loadedItems, safeIndex);
    }
  };

  const handleRemove = async (item: PlaylistItemInfo) => {
    try {
      await api.removePlaylistItem(item.id);
      playlistsChanged();
    } catch (e) {
      toast(String(e));
    }
  };

  const handleMove = async (item: PlaylistItemInfo, position: number) => {
    if (!playlist) return;
    try {
      await api.movePlaylistItem(playlist.id, item.id, position);
      playlistsChanged();
    } catch (e) {
      toast(String(e));
    }
  };

  const openRenameDialog = async () => {
    if (!playlist) return;
    try {
      const all = await api.playlists();
      setExistingNames(
        all
          .map((p) => p.name)
          .filter((n) => n.toLowerCase() !== playlist.name.toLowerCase()),
      );
    } catch {
      setExistingNames([]);
    }
    setRenaming(true);
  };

  const handleRename = async (name: string) => {
    if (!playlist) return;
    try {
      await api.renamePlaylist(playlist.id, name);
      playlistsChanged();
      setRenaming(false);
    } catch (e) {
      toast(String(e));
      throw e;
    }
  };

  const handleDelete = async () => {
    if (!playlist) return;
    try {
      await api.deletePlaylist(playlist.id);
      playlistsChanged();
      push({ name: "personal" });
    } catch (e) {
      toast(String(e));
    }
  };

  const handleExport = async () => {
    if (!playlist) return;
    try {
      const path = await saveDialog({
        defaultPath: `${playlist.name}.jwlplaylist`,
        filters: [{ name: "JW Library playlist", extensions: ["jwlplaylist"] }],
      });
      if (path) {
        await api.exportPlaylist(playlist.id, path);
        toast(t("Exported {name}", { name: playlist.name }));
      }
    } catch (e) {
      toast(t("Export failed: {error}", { error: String(e) }));
    }
  };

  if (!playlist) {
    return <div className="page flex-1" />;
  }

  return (
    <div className="page flex-1 flex flex-col">
      <AppBar title={playlist.name}>
        <div className="flex items-center gap-1">
          <BarButton label={t("Rename")} onClick={() => void openRenameDialog()}>
            <Pencil size={18} strokeWidth={1.8} />
          </BarButton>
          <BarButton label={t("Delete")} onClick={() => setDeleting(true)}>
            <Trash2 size={18} strokeWidth={1.8} />
          </BarButton>
          <BarButton label={t("Export")} onClick={() => void handleExport()}>
            <Share2 size={18} strokeWidth={1.8} />
          </BarButton>
        </div>
      </AppBar>

      <div className="flex-1 overflow-y-auto p-4">
        <div className="mb-4">
          <Button
            onClick={() => void handlePlayAll()}
            disabled={availableItems.length === 0}
            className="flex items-center gap-2"
          >
            <Play size={16} /> {t("Play all")}
          </Button>
        </div>

        {playlist.items.length === 0 ? (
          <p className="py-6 text-center text-sm text-muted">
            {t("Add items to listen to or to watch")}
          </p>
        ) : (
          <table className="w-full border-collapse">
            <tbody>
              {playlist.items.map((item, index) => {
                const available = isItemAvailable(item);
                const thumbnail = item.thumbnail ? convertFileSrc(item.thumbnail) : null;
                const duration = formatDuration(item.durationTicks / TICKS_PER_SECOND);

                return (
                  <tr
                    key={item.id}
                    role="row"
                    onClick={() => available && void handleItemClick(item)}
                    className={cn(
                      "group border-b border-line last:border-b-0 hover:bg-bar transition-colors",
                      available ? "cursor-pointer" : "opacity-60 cursor-not-allowed",
                    )}
                  >
                    <td className="w-14 p-2">
                      {thumbnail ? (
                        <img src={thumbnail} alt="" className="size-12 rounded object-cover" />
                      ) : (
                        <div className="flex size-12 items-center justify-center rounded border border-line bg-surface">
                          <ListMusic size={20} className="text-muted" />
                        </div>
                      )}
                    </td>
                    <td className="min-w-0 p-2">
                      <div className="truncate text-sm font-medium">{item.label}</div>
                      {!available && (
                        <span className="text-xs text-destructive">{t("Unavailable")}</span>
                      )}
                    </td>
                    <td className="w-20 p-2 text-right text-xs text-muted">{duration}</td>
                    <td className="w-10 p-2 text-right" onClick={(e) => e.stopPropagation()}>
                      <DropdownMenu modal={false}>
                        <DropdownMenuTrigger asChild>
                          <button
                            aria-label={t("More")}
                            title={t("More")}
                            className={cn(
                              "inline-flex size-[26px] items-center justify-center rounded-md border border-line bg-surface text-fg",
                              "opacity-0 transition-opacity hover:bg-bar motion-reduce:transition-none",
                              "group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100 aria-expanded:opacity-100",
                              "focus-visible:outline-2 focus-visible:outline-accent [@media(pointer:coarse)]:opacity-100",
                            )}
                          >
                            <MoreHorizontal size={14} strokeWidth={1.8} />
                          </button>
                        </DropdownMenuTrigger>
                        <DropdownMenuContent align="end" className="min-w-[160px]">
                          {index > 0 && (
                            <DropdownMenuItem onSelect={() => void handleMove(item, index - 1)}>
                              {t("Move up")}
                            </DropdownMenuItem>
                          )}
                          {index < playlist.items.length - 1 && (
                            <DropdownMenuItem onSelect={() => void handleMove(item, index + 1)}>
                              {t("Move down")}
                            </DropdownMenuItem>
                          )}
                          <DropdownMenuItem
                            onSelect={() => void handleRemove(item)}
                            className="text-destructive focus:text-destructive"
                          >
                            {t("Remove")}
                          </DropdownMenuItem>
                        </DropdownMenuContent>
                      </DropdownMenu>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}

        {/* Task 13: Import File drop zone slot */}
        <div id="playlist-drop-zone-slot" />
      </div>

      <NameDialog
        open={renaming}
        title={t("Rename playlist")}
        actionLabel={t("Save")}
        initialName={playlist.name}
        existingNames={existingNames}
        onClose={() => setRenaming(false)}
        onSubmit={handleRename}
      />

      <DeleteConfirm
        open={deleting}
        name={playlist.name}
        onClose={() => setDeleting(false)}
        onConfirm={handleDelete}
      />
    </div>
  );
}
