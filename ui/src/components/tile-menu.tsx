import { useCallback, useEffect, useMemo, useState } from "react";
import { Check, ListMusic, MoreHorizontal, Plus } from "lucide-react";
import { useApp } from "@/app";
import {
  api,
  samePublication,
  type PublicationKey,
  type CatalogEntry,
  type Language,
  type LanguageEntry,
  type MediaRef,
  type PlaylistSummary,
  type PubCard,
} from "@/lib/api";
import { loadLanguages } from "@/components/language-menu";
import { NameDialog } from "@/components/playlists";
import { Button } from "@/components/ui/button";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Command, CommandInput, CommandItem, CommandList } from "@/components/ui/command";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { isMediaUrl, mediaBlobUrl } from "@/lib/media";
import { parseMediaKey } from "@/lib/playlist";
import { makeThumbnail } from "@/lib/thumbnail";
import { mediaLink, publicationLink } from "@/lib/share";
import { cn } from "@/lib/utils";
import { t } from "@/lib/i18n";

export interface PubTarget extends PublicationKey {
  kind: "pub";
  symbol: string;
  langCode: string;
  /** Library directory when downloaded. */
  local: string | null;
  title: string;
}
export interface MediaTarget {
  kind: "media";
  key: string;
  langCode: string;
  title: string;
  /** Length in seconds, null when unknown. */
  duration: number | null;
  /** Cover image (`jwmedia:` or plain URL), null when there is none. */
  image: string | null;
}
export type TileTarget = PubTarget | MediaTarget;

/** The menu target of a catalog entry listed in the selected language. */
export const entryTarget = (e: CatalogEntry, lang: string): PubTarget => ({
  kind: "pub",
  symbol: e.item.symbol,
  keySymbol: e.item.key_symbol,
  issueTag: e.item.issue_tag,
  mepsLanguage: e.item.meps_language,
  langCode: lang,
  local: e.local,
  title: e.item.issue_title || e.item.title,
});

/** The menu target of a downloaded publication. */
export const cardTarget = (p: PubCard, lang: string): PubTarget => ({
  kind: "pub",
  symbol: p.symbol,
  keySymbol: p.keySymbol,
  issueTag: Number(p.issueTag) || 0,
  mepsLanguage: p.mepsLanguage,
  langCode: p.langCode ?? lang,
  local: p.dir,
  title: p.shortTitle ?? p.title,
});

/** Languages a publication is available in; picking one opens it, downloading it first if needed. */
function LanguagePicker({ target, onClose }: { target: PubTarget; onClose: () => void }) {
  const { push, download } = useApp();
  const [rows, setRows] = useState<LanguageEntry[] | null | undefined>(undefined);
  const [names, setNames] = useState<Language[]>([]);
  const [query, setQuery] = useState("");

  useEffect(() => {
    api
      .catalogLanguages(target.symbol, target.issueTag)
      .then(setRows)
      .catch(() => setRows(null));
    loadLanguages()
      .then(setNames)
      .catch(() => undefined);
  }, [target.symbol, target.issueTag]);

  const label = useCallback(
    (code: string) => {
      const l = names.find((n) => n.code === code);
      return l ? l.vernacular || l.name : code;
    },
    [names],
  );
  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    return (rows ?? [])
      .map((r) => ({ ...r, name: label(r.code) }))
      .filter((r) => !q || r.name.toLowerCase().includes(q) || r.code.toLowerCase() === q)
      .sort((a, b) => a.name.localeCompare(b.name));
  }, [rows, query, label]);

  const pick = async ({ code, entry }: LanguageEntry) => {
    onClose();
    if (entry.local) push({ name: "publication", dir: entry.local });
    else await download(entry, code);
  };

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent aria-describedby={undefined} className="max-h-[70vh] w-[min(90vw,320px)] gap-0 overflow-hidden p-0">
        <DialogTitle className="sr-only">{t("More Languages")}</DialogTitle>
        {/* Filtered here rather than by cmdk, so a language is found by its name or its code. */}
        <Command shouldFilter={false} label={t("More Languages")}>
          <CommandInput autoFocus value={query} onValueChange={setQuery} placeholder={t("Search languages")} />
          <CommandList className="pb-2">
            {rows === undefined && <div className="px-4 py-2 text-sm text-muted">{t("Loading…")}</div>}
            {rows === null && <div className="px-4 py-2 text-sm text-muted">{t("Load the catalog to see more languages.")}</div>}
            {shown.map((r) => (
              <CommandItem key={r.code} value={r.code} onSelect={() => void pick(r)} className="justify-between">
                {r.name}
                {r.entry.local && <Check size={15} />}
              </CommandItem>
            ))}
          </CommandList>
        </Command>
      </DialogContent>
    </Dialog>
  );
}

/** Makes the playlist thumbnail of a new item from the recording's cover. Failures leave the item without one. */
async function setThumbnail(item: number, image: string, done: () => void) {
  try {
    const url = isMediaUrl(image) ? await mediaBlobUrl(image) : image;
    const jpeg = await makeThumbnail(url, "image");
    if (!jpeg) return;
    await api.setPlaylistItemThumbnail(item, jpeg);
    done();
  } catch {
    // The item simply has no thumbnail.
  }
}

/** Picks a playlist, or creates one, and adds the recording to it. */
function AddToPlaylistDialog({ target, media, onClose }: { target: MediaTarget; media: MediaRef; onClose: () => void }) {
  const { toast, playlistsChanged } = useApp();
  const [playlists, setPlaylists] = useState<PlaylistSummary[] | null>(null);
  const [query, setQuery] = useState("");
  const [creating, setCreating] = useState(false);

  useEffect(() => {
    api
      .playlists()
      .then(setPlaylists)
      .catch((e) => {
        setPlaylists([]);
        toast(String(e));
      });
  }, [toast]);

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    return (playlists ?? []).filter((p) => !q || p.name.toLowerCase().includes(q));
  }, [playlists, query]);

  /** Adds the recording; throws after showing the error. */
  const add = async (id: number, name: string) => {
    try {
      const item = await api.addLibraryPlaylistItem(id, media, target.title, target.duration);
      toast(t("Added to {playlist}", { playlist: name }));
      playlistsChanged();
      if (target.image) void setThumbnail(item, target.image, playlistsChanged);
    } catch (e) {
      toast(String(e));
      throw e;
    }
  };

  const pick = (p: PlaylistSummary) => {
    onClose();
    void add(p.id, p.name).catch(() => undefined);
  };

  const create = async (name: string) => {
    let id: number;
    try {
      id = await api.createPlaylist(name);
    } catch (e) {
      toast(String(e));
      throw e;
    }
    playlistsChanged();
    onClose();
    await add(id, name).catch(() => undefined);
  };

  if (creating) {
    return (
      <NameDialog
        open
        title={t("Create Playlist")}
        existingNames={(playlists ?? []).map((p) => p.name)}
        onClose={() => setCreating(false)}
        onSubmit={create}
      />
    );
  }

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent aria-describedby={undefined} className="max-h-[70vh] w-[min(90vw,360px)] gap-0 overflow-hidden p-0">
        <DialogTitle className="px-4 pb-2 pt-4 text-base">{t("Add to Playlist")}</DialogTitle>
        <div className="px-4 pb-2">
          <Button variant="outline" size="sm" className="w-full" onClick={() => setCreating(true)}>
            <Plus size={15} /> {t("Create Playlist")}
          </Button>
        </div>
        <Command shouldFilter={false} label={t("Add to Playlist")}>
          <CommandInput autoFocus value={query} onValueChange={setQuery} placeholder={t("Search playlists")} />
          <CommandList className="pb-2">
            {playlists === null && <div className="px-4 py-2 text-sm text-muted">{t("Loading…")}</div>}
            {playlists?.length === 0 && <div className="px-4 py-2 text-sm text-muted">{t("No playlists yet")}</div>}
            {shown.map((p) => (
              <CommandItem key={p.id} value={String(p.id)} onSelect={() => pick(p)}>
                <ListMusic size={15} />
                <span className="min-w-0 flex-1 truncate">{p.name}</span>
                <span className="text-xs text-muted">{t("{n} items", { n: p.items })}</span>
              </CommandItem>
            ))}
          </CommandList>
        </Command>
      </DialogContent>
    </Dialog>
  );
}

/** Asks before a downloaded publication is deleted. */
function RemoveConfirm({ title, onConfirm, onClose }: { title: string; onConfirm: () => void; onClose: () => void }) {
  return (
    <AlertDialog open onOpenChange={(o) => !o && onClose()}>
      <AlertDialogContent>
        <AlertDialogTitle className="sr-only">{t("Remove from library")}</AlertDialogTitle>
        <AlertDialogDescription>{t("Remove {title} from the library?", { title })}</AlertDialogDescription>
        <AlertDialogFooter>
          <AlertDialogCancel>{t("Cancel")}</AlertDialogCancel>
          <AlertDialogAction onClick={onConfirm}>{t("Remove")}</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

/** A 26px square control on a cover or row: hidden until it is hovered or focused, always shown on touch screens. */
export const coverControl = cn(
  "inline-flex size-[26px] items-center justify-center rounded-md border border-line bg-surface text-fg",
  "opacity-0 transition-opacity hover:bg-bar motion-reduce:transition-none",
  "group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100 aria-expanded:opacity-100",
  "focus-visible:outline-2 focus-visible:outline-accent [@media(pointer:coarse)]:opacity-100",
);

/** The 3-dots button of a tile and its menu. The tile must be `group relative`; this sits in its top-right corner. */
export function TileMenu({
  target,
  className,
}: {
  target: TileTarget;
  className?: string;
}) {
  const { toast, favorites, setFavorite, removePublication } = useApp();
  const [picking, setPicking] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [adding, setAdding] = useState(false);
  const media = target.kind === "media" ? parseMediaKey(target.key) : null;
  const favorite = target.kind === "pub" && favorites.some((key) => samePublication(key, target));

  const share = async () => {
    const link =
      target.kind === "pub"
        ? publicationLink(target.symbol, target.issueTag, target.langCode)
        : mediaLink(target.key, target.langCode);
    try {
      await navigator.clipboard.writeText(link);
      toast(t("Link copied"));
    } catch (e) {
      toast(t("Copy failed: {error}", { error: String(e) }));
    }
  };

  return (
    <>
      {/* Not modal, so a dialog opened from it gets the pointer and the focus. */}
      <DropdownMenu modal={false}>
        <DropdownMenuTrigger asChild>
          <button
            aria-label={t("More")}
            title={t("More")}
            // The tile under it opens on click.
            onClick={(e) => e.stopPropagation()}
            className={cn(coverControl, "absolute right-1.5 top-1.5", className)}
          >
            <MoreHorizontal size={14} strokeWidth={1.8} />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="min-w-[200px]" onClick={(e) => e.stopPropagation()}>
          <DropdownMenuItem onSelect={() => void share()}>{t("Share Link")}</DropdownMenuItem>
          {media && <DropdownMenuItem onSelect={() => setAdding(true)}>{t("Add to Playlist")}</DropdownMenuItem>}
          {target.kind === "pub" && <DropdownMenuItem onSelect={() => setPicking(true)}>{t("More Languages")}</DropdownMenuItem>}
          {target.kind === "pub" && (
            <DropdownMenuItem onSelect={() => void setFavorite(target, !favorite)}>
              {favorite ? t("Remove from Favorites") : t("Add to Favorites")}
            </DropdownMenuItem>
          )}
          {target.kind === "pub" && target.local && (
            <DropdownMenuItem onSelect={() => setConfirming(true)}>{t("Remove")}</DropdownMenuItem>
          )}
        </DropdownMenuContent>
      </DropdownMenu>
      {picking && target.kind === "pub" && <LanguagePicker target={target} onClose={() => setPicking(false)} />}
      {adding && target.kind === "media" && media && (
        <AddToPlaylistDialog target={target} media={media} onClose={() => setAdding(false)} />
      )}
      {confirming && target.kind === "pub" && target.local && (
        <RemoveConfirm
          title={target.title}
          onClose={() => setConfirming(false)}
          onConfirm={() => {
            setConfirming(false);
            void removePublication(target.local!, target.title);
          }}
        />
      )}
    </>
  );
}
