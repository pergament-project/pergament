/** File extensions supported in playlists, grouped by kind. */
export const PLAYLIST_FILE_TYPES = {
  video: ["mp4", "m4v", "mov"] as const,
  photo: ["jpg", "jpeg", "png", "gif", "webp", "heic"] as const,
  audio: ["mp3", "m4a", "aac", "wav"] as const,
};

/** All supported extensions as a flat array. */
export const ALL_PLAYLIST_EXTENSIONS = [
  ...PLAYLIST_FILE_TYPES.video,
  ...PLAYLIST_FILE_TYPES.photo,
  ...PLAYLIST_FILE_TYPES.audio,
] as const;

/** Checks if a file path has a supported playlist extension (case-insensitive). */
export function isPlaylistFile(path: string): boolean {
  const ext = path.toLowerCase().split(".").pop() ?? "";
  return ALL_PLAYLIST_EXTENSIONS.includes(ext as (typeof ALL_PLAYLIST_EXTENSIONS)[number]);
}

/**
 * Creates a JPEG thumbnail from an image or video URL.
 * Longest side ≤ 640 px, aspect ratio preserved.
 * Returns null if the webview cannot decode the file.
 */
export async function makeThumbnail(
  url: string,
  kind: "image" | "video",
): Promise<Uint8Array | null> {
  try {
    if (kind === "image") {
      return await makeImageThumbnail(url);
    } else {
      return await makeVideoThumbnail(url);
    }
  } catch {
    return null;
  }
}

/** How long a thumbnail or duration probe may take before it gives up and resolves null. */
const MEDIA_TIMEOUT_MS = 10_000;

/**
 * Detaches the handlers first, so the reset below cannot fire them, then drops the source
 * so the webview releases the file. The element is never appended, but remove it if it was.
 */
function releaseMedia(element: HTMLImageElement | HTMLMediaElement): void {
  const media = "load" in element ? element : null;
  element.onload = null;
  element.onerror = null;
  if (media) {
    media.onloadedmetadata = null;
    media.onseeked = null;
  }
  element.removeAttribute("src");
  media?.load();
  element.remove();
}

/**
 * Wraps a media probe so it resolves exactly once: on the first result, or null after the timeout.
 * The element is released on every path.
 */
function settleOnce<T>(
  element: HTMLImageElement | HTMLMediaElement,
  resolve: (value: T | null) => void,
): (value: T | null) => void {
  let settled = false;
  const finish = (value: T | null) => {
    if (settled) return;
    settled = true;
    clearTimeout(timeout);
    try {
      releaseMedia(element);
    } catch {
      // Releasing is best effort; the result must still reach the caller.
    }
    resolve(value);
  };
  const timeout = setTimeout(() => finish(null), MEDIA_TIMEOUT_MS);
  return finish;
}

/** Draws a source onto a canvas of the given size and encodes it as JPEG bytes. */
async function encodeThumbnail(
  source: CanvasImageSource,
  width: number,
  height: number,
): Promise<Uint8Array | null> {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;

  const ctx = canvas.getContext("2d");
  if (!ctx) return null;

  ctx.drawImage(source, 0, 0, width, height);

  const blob = await canvasToBlob(canvas);
  if (!blob) return null;

  return new Uint8Array(await blob.arrayBuffer());
}

/** Creates a thumbnail from an image URL. */
async function makeImageThumbnail(url: string): Promise<Uint8Array | null> {
  return new Promise((resolve) => {
    const img = new Image();
    const finish = settleOnce<Uint8Array>(img, resolve);
    img.crossOrigin = "anonymous";

    img.onload = async () => {
      try {
        let bitmap: ImageBitmap | null = null;
        if (typeof createImageBitmap === "function") {
          try {
            bitmap = await createImageBitmap(img);
          } catch {
            bitmap = null;
          }
        }

        const sourceWidth = bitmap?.width ?? img.width;
        const sourceHeight = bitmap?.height ?? img.height;
        const { width, height } = calculateThumbnailSize(sourceWidth, sourceHeight);

        finish(await encodeThumbnail(bitmap ?? img, width, height));
      } catch {
        finish(null);
      }
    };

    img.onerror = () => finish(null);
    img.src = url;
  });
}

/** Creates a thumbnail from a video URL by seeking to 1s (or duration/2 or 0). */
async function makeVideoThumbnail(url: string): Promise<Uint8Array | null> {
  return new Promise((resolve) => {
    const video = document.createElement("video");
    const finish = settleOnce<Uint8Array>(video, resolve);
    video.crossOrigin = "anonymous";
    video.preload = "metadata";

    video.onloadedmetadata = () => {
      const duration = video.duration || 0;
      let seekTime: number;
      if (duration >= 1) {
        seekTime = 1;
      } else if (duration > 0) {
        seekTime = duration / 2;
      } else {
        seekTime = 0;
      }

      video.currentTime = seekTime;
    };

    video.onseeked = async () => {
      try {
        if (video.videoWidth === 0 || video.videoHeight === 0) {
          finish(null);
          return;
        }

        const { width, height } = calculateThumbnailSize(video.videoWidth, video.videoHeight);
        finish(await encodeThumbnail(video, width, height));
      } catch {
        finish(null);
      }
    };

    video.onerror = () => finish(null);
    video.src = url;
  });
}

/** Calculates thumbnail dimensions with longest side ≤ 640, preserving aspect ratio. */
function calculateThumbnailSize(sourceWidth: number, sourceHeight: number): { width: number; height: number } {
  const MAX_SIDE = 640;
  if (sourceWidth >= sourceHeight) {
    const width = Math.min(sourceWidth, MAX_SIDE);
    const height = Math.round((sourceHeight * width) / sourceWidth);
    return { width, height };
  } else {
    const height = Math.min(sourceHeight, MAX_SIDE);
    const width = Math.round((sourceWidth * height) / sourceHeight);
    return { width, height };
  }
}

/** Converts canvas to JPEG blob with quality 0.85. */
function canvasToBlob(canvas: HTMLCanvasElement): Promise<Blob | null> {
  return new Promise((resolve) => {
    canvas.toBlob(
      (blob) => {
        resolve(blob);
      },
      "image/jpeg",
      0.85,
    );
  });
}

/**
 * Measures the duration of an audio or video file in seconds.
 * Returns null on error or timeout (10 seconds).
 */
export async function mediaDuration(
  url: string,
  kind: "audio" | "video",
): Promise<number | null> {
  return new Promise((resolve) => {
    const element = document.createElement(kind);
    const finish = settleOnce<number>(element, resolve);
    element.preload = "metadata";

    element.onloadedmetadata = () => {
      const duration = element.duration;
      finish(Number.isFinite(duration) && duration > 0 ? duration : null);
    };

    element.onerror = () => finish(null);
    element.src = url;
  });
}
