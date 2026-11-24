import { useCallback, useEffect, useState } from "react";
import { ListMusic, Plus, MoreHorizontal } from "lucide-react";
import { SectionTitle, useApp } from "@/app";
import { api, type PlaylistSummary } from "@/lib/api";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Input } from "@/components/ui/input";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { convertFileSrc } from "@tauri-apps/api/core";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { coverControl } from "@/components/tile-menu";
import { t } from "@/lib/i18n";

/** Catalog + states, reloaded on mount, on `userVersion` and `playlistsVersion` and after every write it performs. */
export function usePlaylists() {
  const { toast, userVersion, playlistsVersion, playlistsChanged } = useApp();
  const [playlists, setPlaylists] = useState<PlaylistSummary[]>([]);

  const load = useCallback(
    () =>
      api
        .playlists()
        .then(setPlaylists)
        .catch((e) => toast(String(e))),
    [toast],
  );

  useEffect(() => {
    void load();
  }, [load, userVersion, playlistsVersion]);

  const write = useCallback(
    async (run: () => Promise<unknown>) => {
      try {
        await run();
        playlistsChanged();
      } catch (e) {
        toast(String(e));
        throw e;
      }
    },
    [toast, playlistsChanged],
  );

  return {
    playlists,
    create: (name: string) => write(() => api.createPlaylist(name)),
    rename: (id: number, name: string) => write(() => api.renamePlaylist(id, name)),
    remove: (id: number) => write(() => api.deletePlaylist(id)),
    reload: load,
  };
}

/** Dialog to create or rename a playlist with client & backend validation. */
export function NameDialog({
  open,
  title,
  initialName = "",
  existingNames = [],
  actionLabel,
  onClose,
  onSubmit,
}: {
  open: boolean;
  title: string;
  initialName?: string;
  existingNames?: string[];
  actionLabel?: string;
  onClose: () => void;
  onSubmit: (name: string) => Promise<void>;
}) {
  const [name, setName] = useState(initialName);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (open) {
      setName(initialName);
      setError("");
      setBusy(false);
    }
  }, [open, initialName]);

  const submit = async () => {
    const trimmed = name.trim();
    if (!trimmed) {
      setError(t("Enter a name"));
      return;
    }
    if (existingNames.some((n) => n.toLowerCase() === trimmed.toLowerCase())) {
      setError(t("A playlist with this name already exists"));
      return;
    }
    setError("");
    setBusy(true);
    try {
      await onSubmit(trimmed);
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent aria-describedby={undefined}>
        <DialogTitle>{title}</DialogTitle>
        {error && <DialogDescription className="text-destructive text-sm">{error}</DialogDescription>}
        <label className="flex flex-col gap-1 text-sm">
          {t("Name")}
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && void submit()}
            autoFocus
            placeholder={t("New playlist")}
          />
        </label>
        <div className="flex justify-end gap-2">
          <Button variant="outline" size="sm" onClick={onClose}>
            {t("Cancel")}
          </Button>
          <Button size="sm" disabled={busy} onClick={() => void submit()}>
            {actionLabel ?? t("Create")}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

/** Delete confirmation dialog. */
export function DeleteConfirm({
  open,
  name,
  onClose,
  onConfirm,
}: {
  open: boolean;
  name: string;
  onClose: () => void;
  onConfirm: () => Promise<void> | void;
}) {
  return (
    <AlertDialog open={open} onOpenChange={(o) => !o && onClose()}>
      <AlertDialogContent>
        <AlertDialogTitle className="sr-only">{t("Delete playlist")}</AlertDialogTitle>
        <AlertDialogDescription>{t("Delete playlist {name}?", { name })}</AlertDialogDescription>
        <AlertDialogFooter>
          <AlertDialogCancel>{t("Cancel")}</AlertDialogCancel>
          <AlertDialogAction onClick={() => void onConfirm()}>{t("Delete")}</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

/** The ⋯ menu of a playlist card. */
function PlaylistCardMenu({
  onRename,
  onExport,
  onDelete,
}: {
  onRename: () => void;
  onExport: () => void;
  onDelete: () => void;
}) {
  return (
    <DropdownMenu modal={false}>
      <DropdownMenuTrigger asChild>
        <button
          aria-label={t("More")}
          title={t("More")}
          onClick={(e) => e.stopPropagation()}
          className={cn(coverControl, "absolute right-2 top-2")}
        >
          <MoreHorizontal size={14} strokeWidth={1.8} />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="min-w-[180px]" onClick={(e) => e.stopPropagation()}>
        <DropdownMenuItem onSelect={onRename}>{t("Rename")}</DropdownMenuItem>
        <DropdownMenuItem onSelect={onExport}>{t("Export…")}</DropdownMenuItem>
        <DropdownMenuItem onSelect={onDelete} className="text-destructive focus:text-destructive">
          {t("Delete")}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** One playlist card in the Personal Study section. */
export function PlaylistCard({
  playlist,
  onClick,
  onRename,
  onExport,
  onDelete,
}: {
  playlist: PlaylistSummary;
  onClick: () => void;
  onRename: () => void;
  onExport: () => void;
  onDelete: () => void;
}) {
  const thumbnail = playlist.thumbnail ? convertFileSrc(playlist.thumbnail) : null;
  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onClick}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onClick();
        }
      }}
      className="group relative flex cursor-pointer items-center gap-3 rounded-md border border-line p-3 hover:bg-bar"
    >
      {thumbnail ? (
        <img src={thumbnail} alt="" className="size-14 shrink-0 rounded object-cover" />
      ) : (
        <div className="flex size-14 shrink-0 items-center justify-center rounded border border-line bg-surface">
          <ListMusic size={24} className="text-muted" />
        </div>
      )}
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm font-medium">{playlist.name}</div>
        <div className="text-[13px] text-muted">{t("{n} items", { n: playlist.items })}</div>
      </div>
      <PlaylistCardMenu onRename={onRename} onExport={onExport} onDelete={onDelete} />
    </div>
  );
}

export function PlaylistsSection() {
  const { push, toast, playlistsChanged } = useApp();
  const { playlists, create, rename, remove } = usePlaylists();
  const [creating, setCreating] = useState(false);
  const [renaming, setRenaming] = useState<PlaylistSummary | null>(null);
  const [deleting, setDeleting] = useState<PlaylistSummary | null>(null);

  const existingNames = playlists.map((p) => p.name);

  const handleCreate = async (name: string) => {
    await create(name);
  };

  const handleRename = async (name: string) => {
    if (renaming) {
      await rename(renaming.id, name);
    }
  };

  const handleDelete = async () => {
    if (deleting) {
      await remove(deleting.id);
      setDeleting(null);
    }
  };

  const handleExport = async (p: PlaylistSummary) => {
    try {
      const path = await saveDialog({
        defaultPath: `${p.name}.jwlplaylist`,
        filters: [{ name: "JW Library playlist", extensions: ["jwlplaylist"] }],
      });
      if (path) {
        await api.exportPlaylist(p.id, path);
        toast(t("Exported {name}", { name: p.name }));
      }
    } catch (e) {
      toast(t("Export failed: {error}", { error: String(e) }));
    }
  };

  const handleImport = async () => {
    try {
      const picked = await openDialog({
        filters: [{ name: "JW Library playlist", extensions: ["jwlplaylist"] }],
        multiple: false,
      });
      if (!picked || Array.isArray(picked)) return;
      const id = await api.importPlaylist(picked);
      playlistsChanged();
      push({ name: "playlist", id });
    } catch (e) {
      toast(t("Import failed: {error}", { error: String(e) }));
    }
  };

  return (
    <>
      <SectionTitle
        aside={
          <DropdownMenu modal={false}>
            <DropdownMenuTrigger asChild>
              <button
                aria-label={t("Add")}
                title={t("Add")}
                className="flex size-7 items-center justify-center rounded-md border border-line bg-surface text-fg hover:bg-bar"
              >
                <Plus size={16} />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="min-w-[160px]">
              <DropdownMenuItem onSelect={() => setCreating(true)}>{t("Create Playlist")}</DropdownMenuItem>
              <DropdownMenuItem onSelect={() => void handleImport()}>{t("Import Playlist…")}</DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        }
      >
        {t("Playlists")}
      </SectionTitle>

      {playlists.length === 0 && <p className="mb-3 text-sm text-muted">{t("No playlists yet")}</p>}

      <div className="flex flex-col gap-2">
        {playlists.map((p) => (
          <PlaylistCard
            key={p.id}
            playlist={p}
            onClick={() => push({ name: "playlist", id: p.id })}
            onRename={() => setRenaming(p)}
            onExport={() => void handleExport(p)}
            onDelete={() => setDeleting(p)}
          />
        ))}
      </div>

      <NameDialog
        open={creating}
        title={t("Create Playlist")}
        actionLabel={t("Create")}
        existingNames={existingNames}
        onClose={() => setCreating(false)}
        onSubmit={handleCreate}
      />

      <NameDialog
        open={renaming !== null}
        title={t("Rename playlist")}
        actionLabel={t("Save")}
        initialName={renaming?.name ?? ""}
        existingNames={existingNames.filter((n) => n.toLowerCase() !== renaming?.name.toLowerCase())}
        onClose={() => setRenaming(null)}
        onSubmit={handleRename}
      />

      <DeleteConfirm
        open={deleting !== null}
        name={deleting?.name ?? ""}
        onClose={() => setDeleting(null)}
        onConfirm={handleDelete}
      />
    </>
  );
}
