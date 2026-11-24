// @vitest-environment jsdom
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";

import { PLAYLIST_FILE_TYPES, isPlaylistFile, makeThumbnail, mediaDuration, ALL_PLAYLIST_EXTENSIONS } from "./thumbnail";

describe("PLAYLIST_FILE_TYPES", () => {
  it("has the correct video extensions", () => {
    expect(PLAYLIST_FILE_TYPES.video).toEqual(["mp4", "m4v", "mov"]);
  });

  it("has the correct photo extensions", () => {
    expect(PLAYLIST_FILE_TYPES.photo).toEqual(["jpg", "jpeg", "png", "gif", "webp", "heic"]);
  });

  it("has the correct audio extensions", () => {
    expect(PLAYLIST_FILE_TYPES.audio).toEqual(["mp3", "m4a", "aac", "wav"]);
  });
});

describe("ALL_PLAYLIST_EXTENSIONS", () => {
  it("contains all extensions from all categories", () => {
    expect(ALL_PLAYLIST_EXTENSIONS).toEqual([
      "mp4", "m4v", "mov",
      "jpg", "jpeg", "png", "gif", "webp", "heic",
      "mp3", "m4a", "aac", "wav",
    ]);
  });
});

describe("isPlaylistFile", () => {
  it("returns true for supported video extensions (case-insensitive)", () => {
    expect(isPlaylistFile("test.mp4")).toBe(true);
    expect(isPlaylistFile("test.MP4")).toBe(true);
    expect(isPlaylistFile("test.m4v")).toBe(true);
    expect(isPlaylistFile("test.MOV")).toBe(true);
    expect(isPlaylistFile("a.MOV")).toBe(true);
  });

  it("returns true for supported photo extensions (case-insensitive)", () => {
    expect(isPlaylistFile("test.jpg")).toBe(true);
    expect(isPlaylistFile("test.JPEG")).toBe(true);
    expect(isPlaylistFile("test.png")).toBe(true);
    expect(isPlaylistFile("test.gif")).toBe(true);
    expect(isPlaylistFile("test.webp")).toBe(true);
    expect(isPlaylistFile("test.heic")).toBe(true);
    expect(isPlaylistFile("b.heic")).toBe(true);
  });

  it("returns true for supported audio extensions (case-insensitive)", () => {
    expect(isPlaylistFile("test.mp3")).toBe(true);
    expect(isPlaylistFile("test.m4a")).toBe(true);
    expect(isPlaylistFile("test.aac")).toBe(true);
    expect(isPlaylistFile("test.wav")).toBe(true);
    expect(isPlaylistFile("c.m4a")).toBe(true);
  });

  it("returns false for unsupported extensions", () => {
    expect(isPlaylistFile("test.pdf")).toBe(false);
    expect(isPlaylistFile("test.txt")).toBe(false);
    expect(isPlaylistFile("test.doc")).toBe(false);
    expect(isPlaylistFile("d.pdf")).toBe(false);
  });

  it("returns false for files without extension", () => {
    expect(isPlaylistFile("noext")).toBe(false);
    expect(isPlaylistFile("")).toBe(false);
  });
});

describe("makeThumbnail", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("is a function that can be imported", () => {
    expect(typeof makeThumbnail).toBe("function");
  });

  it("returns a Promise", () => {
    const result = makeThumbnail("http://example.com/image.jpg", "image");
    expect(result).toBeInstanceOf(Promise);
  });

  it("scales a 1920x1080 image to 640x360 and returns JPEG blob from canvas.toBlob", async () => {
    const mockBlob = new Blob(["test"], { type: "image/jpeg" });
    const mockToBlob = vi.fn((callback) => callback(mockBlob));
    const mockDrawImage = vi.fn();
    const mockGetContext = vi.fn(() => ({
      drawImage: mockDrawImage,
    }));
    let imgOnload = null as (() => void) | null;
    let imgOnerror = null as (() => void) | null;

    // Mock canvas object
    const mockCanvas = {
      width: 0,
      height: 0,
      getContext: mockGetContext,
      toBlob: mockToBlob,
    };

    // Mock Image constructor
    class MockImage {
      crossOrigin = "";
      src = "";
      width = 1920;
      height = 1080;
      get onload() { return imgOnload; }
      set onload(fn: (() => void) | null) { imgOnload = fn; }
      get onerror() { return imgOnerror; }
      set onerror(fn: (() => void) | null) { imgOnerror = fn; }
      removeAttribute = vi.fn();
      remove = vi.fn();
    }
    vi.stubGlobal("Image", MockImage);

    const mockCreateElement = vi.fn((tag) => {
      if (tag === "canvas") {
        return mockCanvas;
      }
      return {};
    }) as unknown as typeof document.createElement;
    vi.spyOn(document, "createElement").mockImplementation(mockCreateElement);

    const promise = makeThumbnail("http://example.com/image.jpg", "image");

    // Trigger the onload callback
    if (imgOnload) imgOnload();
    vi.advanceTimersByTime(100);

    const result = await promise;

    expect(result).toBeInstanceOf(Uint8Array);
    expect(mockCanvas.width).toBe(640);
    expect(mockCanvas.height).toBe(360);
    expect(mockToBlob).toHaveBeenCalledWith(expect.any(Function), "image/jpeg", 0.85);
  });

  it("returns null on image decode error (onerror)", async () => {
    let imgOnload = null as (() => void) | null;
    let imgOnerror = null as (() => void) | null;

    // Mock Image constructor
    class MockImage {
      crossOrigin = "";
      src = "";
      get onload() { return imgOnload; }
      set onload(fn: (() => void) | null) { imgOnload = fn; }
      get onerror() { return imgOnerror; }
      set onerror(fn: (() => void) | null) { imgOnerror = fn; }
      removeAttribute = vi.fn();
      remove = vi.fn();
    }
    vi.stubGlobal("Image", MockImage);

    const mockCreateElement = vi.fn((tag) => {
      if (tag === "canvas") {
        return { width: 0, height: 0, getContext: () => ({ drawImage: vi.fn() }), toBlob: vi.fn() };
      }
      return {};
    }) as unknown as typeof document.createElement;
    vi.spyOn(document, "createElement").mockImplementation(mockCreateElement);

    const promise = makeThumbnail("http://example.com/broken.jpg", "image");

    // Trigger the onerror callback
    if (imgOnerror) imgOnerror();
    vi.advanceTimersByTime(100);

    const result = await promise;
    expect(result).toBeNull();
  });

  it("image thumbnail returns null after 10 seconds and releases the element", async () => {
    const images: { onload: unknown; onerror: unknown; removeAttribute: ReturnType<typeof vi.fn>; remove: ReturnType<typeof vi.fn> }[] = [];
    class MockImage {
      crossOrigin = "";
      src = "";
      onload: (() => void) | null = null;
      onerror: (() => void) | null = null;
      removeAttribute = vi.fn();
      remove = vi.fn();
      constructor() {
        images.push(this);
      }
    }
    vi.stubGlobal("Image", MockImage);

    const promise = makeThumbnail("http://example.com/slow.jpg", "image");
    vi.advanceTimersByTime(10_000);

    expect(await promise).toBeNull();
    const [img] = images;
    expect(img.onload).toBeNull();
    expect(img.onerror).toBeNull();
    expect(img.removeAttribute).toHaveBeenCalledWith("src");
    expect(img.remove).toHaveBeenCalledTimes(1);
  });

  // Video thumbnail tests
  type MockVideo = {
    crossOrigin: string;
    src: string;
    preload: string;
    videoWidth: number;
    videoHeight: number;
    duration: number;
    currentTime: number;
    onloadedmetadata: (() => void) | null;
    onseeked: (() => void) | null;
    onerror: (() => void) | null;
    removeAttribute: ReturnType<typeof vi.fn>;
    load: ReturnType<typeof vi.fn>;
    remove: ReturnType<typeof vi.fn>;
  };

  function mockVideoElement(overrides: Partial<MockVideo> = {}): MockVideo {
    return {
      crossOrigin: "",
      src: "",
      preload: "",
      videoWidth: 1920,
      videoHeight: 1080,
      duration: 10,
      currentTime: 0,
      onloadedmetadata: null,
      onseeked: null,
      onerror: null,
      removeAttribute: vi.fn(),
      load: vi.fn(),
      remove: vi.fn(),
      ...overrides,
    };
  }

  function mockCanvasElement() {
    const mockBlob = new Blob(["test"], { type: "image/jpeg" });
    return {
      width: 0,
      height: 0,
      getContext: vi.fn(() => ({ drawImage: vi.fn() })),
      toBlob: vi.fn((callback: (blob: Blob | null) => void) => callback(mockBlob)),
    };
  }

  /** Routes createElement("video") and createElement("canvas") to the given mocks. */
  function stubVideoElements(video: MockVideo, canvas = mockCanvasElement()) {
    const original = document.createElement.bind(document);
    vi.spyOn(document, "createElement").mockImplementation(((tag: string) => {
      if (tag === "canvas") return canvas as unknown as HTMLElement;
      if (tag === "video") return video as unknown as HTMLElement;
      return original(tag);
    }) as typeof document.createElement);
    return canvas;
  }

  function expectVideoReleased(video: MockVideo) {
    expect(video.onloadedmetadata).toBeNull();
    expect(video.onseeked).toBeNull();
    expect(video.onerror).toBeNull();
    expect(video.removeAttribute).toHaveBeenCalledWith("src");
    expect(video.load).toHaveBeenCalled();
    expect(video.remove).toHaveBeenCalledTimes(1);
  }

  it("video thumbnail seeks to 1s and draws frame for duration >= 1", async () => {
    const video = mockVideoElement({ duration: 10 });
    const canvas = stubVideoElements(video);

    const promise = makeThumbnail("http://example.com/video.mp4", "video");
    expect(video.src).toBe("http://example.com/video.mp4");
    video.onloadedmetadata?.();
    expect(video.currentTime).toBe(1);
    video.onseeked?.();

    const result = await promise;
    expect(result).toBeInstanceOf(Uint8Array);
    expect(canvas.width).toBe(640);
    expect(canvas.height).toBe(360);
    expect(canvas.toBlob).toHaveBeenCalledWith(expect.any(Function), "image/jpeg", 0.85);
    expectVideoReleased(video);
  });

  it("video thumbnail seeks to duration/2 for 0 < duration < 1", async () => {
    const video = mockVideoElement({ duration: 0.5 });
    stubVideoElements(video);

    const promise = makeThumbnail("http://example.com/video.mp4", "video");
    video.onloadedmetadata?.();
    expect(video.currentTime).toBe(0.25);
    video.onseeked?.();

    expect(await promise).toBeInstanceOf(Uint8Array);
  });

  it("video thumbnail seeks to 0 for duration === 0", async () => {
    const video = mockVideoElement({ duration: 0, currentTime: 5 });
    stubVideoElements(video);

    const promise = makeThumbnail("http://example.com/video.mp4", "video");
    video.onloadedmetadata?.();
    expect(video.currentTime).toBe(0);
    video.onseeked?.();

    expect(await promise).toBeInstanceOf(Uint8Array);
  });

  it("video thumbnail returns null when video dimensions are zero", async () => {
    const video = mockVideoElement({ videoWidth: 0, videoHeight: 0 });
    stubVideoElements(video);

    const promise = makeThumbnail("http://example.com/video.mp4", "video");
    video.onloadedmetadata?.();
    video.onseeked?.();

    expect(await promise).toBeNull();
    expectVideoReleased(video);
  });

  it("video thumbnail returns null on video error and releases the element", async () => {
    const video = mockVideoElement();
    stubVideoElements(video);

    const promise = makeThumbnail("http://example.com/video.mp4", "video");
    video.onloadedmetadata?.();
    video.onerror?.();

    expect(await promise).toBeNull();
    expectVideoReleased(video);
  });

  it("video thumbnail returns null after 10 seconds when seeked never fires", async () => {
    const video = mockVideoElement();
    stubVideoElements(video);

    const promise = makeThumbnail("http://example.com/video.mp4", "video");
    video.onloadedmetadata?.();
    vi.advanceTimersByTime(9_999);
    expect(video.remove).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);

    expect(await promise).toBeNull();
    expectVideoReleased(video);
  });

  it("video thumbnail keeps the timeout result when a stale seeked handler runs later", async () => {
    const video = mockVideoElement();
    stubVideoElements(video);

    const promise = makeThumbnail("http://example.com/video.mp4", "video");
    // Release detaches the handler, so only a reference captured beforehand can still run.
    const staleSeeked = video.onseeked;
    vi.advanceTimersByTime(10_000);
    await staleSeeked?.();

    expect(await promise).toBeNull();
    expect(video.remove).toHaveBeenCalledTimes(1);
  });
});

describe("mediaDuration", () => {
  type MockMedia = {
    src: string;
    preload: string;
    duration: number;
    onloadedmetadata: (() => void) | null;
    onerror: (() => void) | null;
    removeAttribute: ReturnType<typeof vi.fn>;
    load: ReturnType<typeof vi.fn>;
    remove: ReturnType<typeof vi.fn>;
  };

  function mockMedia(duration: number): MockMedia {
    return {
      src: "",
      preload: "",
      duration,
      onloadedmetadata: null,
      onerror: null,
      removeAttribute: vi.fn(),
      load: vi.fn(),
      remove: vi.fn(),
    };
  }

  function stubCreateElement(el: MockMedia) {
    const original = document.createElement.bind(document);
    return vi.spyOn(document, "createElement").mockImplementation(((tag: string) => {
      if (tag === "audio" || tag === "video") return el as unknown as HTMLElement;
      return original(tag);
    }) as typeof document.createElement);
  }

  function expectCleanedUp(el: MockMedia) {
    expect(el.onloadedmetadata).toBeNull();
    expect(el.onerror).toBeNull();
    expect(el.removeAttribute).toHaveBeenCalledWith("src");
    expect(el.load).toHaveBeenCalled();
    expect(el.remove).toHaveBeenCalled();
  }

  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("resolves the duration of an audio file and cleans up", async () => {
    const el = mockMedia(123.45);
    const spy = stubCreateElement(el);

    const promise = mediaDuration("http://example.com/audio.mp3", "audio");
    expect(spy).toHaveBeenCalledWith("audio");
    expect(el.src).toBe("http://example.com/audio.mp3");
    expect(el.preload).toBe("metadata");
    el.onloadedmetadata?.();

    await expect(promise).resolves.toBe(123.45);
    expectCleanedUp(el);
  });

  it("resolves the duration of a video file", async () => {
    const el = mockMedia(3600);
    const spy = stubCreateElement(el);

    const promise = mediaDuration("http://example.com/video.mp4", "video");
    expect(spy).toHaveBeenCalledWith("video");
    el.onloadedmetadata?.();

    await expect(promise).resolves.toBe(3600);
    expectCleanedUp(el);
  });

  it("resolves null for a NaN duration", async () => {
    const el = mockMedia(Number.NaN);
    stubCreateElement(el);

    const promise = mediaDuration("http://example.com/audio.mp3", "audio");
    el.onloadedmetadata?.();

    await expect(promise).resolves.toBeNull();
  });

  it("resolves null for a zero duration", async () => {
    const el = mockMedia(0);
    stubCreateElement(el);

    const promise = mediaDuration("http://example.com/audio.mp3", "audio");
    el.onloadedmetadata?.();

    await expect(promise).resolves.toBeNull();
  });

  it("resolves null after 10 seconds and cleans up", async () => {
    const el = mockMedia(42);
    stubCreateElement(el);

    const promise = mediaDuration("http://example.com/audio.mp3", "audio");
    vi.advanceTimersByTime(9_999);
    expect(el.remove).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);

    await expect(promise).resolves.toBeNull();
    expectCleanedUp(el);
  });

  it("resolves null on a media error and cleans up", async () => {
    const el = mockMedia(42);
    stubCreateElement(el);

    const promise = mediaDuration("http://example.com/audio.mp3", "audio");
    el.onerror?.();

    await expect(promise).resolves.toBeNull();
    expectCleanedUp(el);
  });

  it("does not clean up twice when the timer fires after success", async () => {
    const el = mockMedia(5);
    stubCreateElement(el);

    const promise = mediaDuration("http://example.com/audio.mp3", "audio");
    el.onloadedmetadata?.();
    vi.advanceTimersByTime(10_000);

    await expect(promise).resolves.toBe(5);
    expect(el.remove).toHaveBeenCalledTimes(1);
  });
});
