import { useEffect, useState } from "react";
import { api } from "@/lib/ipc";

/**
 * One small cover thumbnail for a similarity-group card. Requests a grid
 * thumbnail by photo id only (the group card knows ids, not file names);
 * undecodable formats fall back to a neutral tile rather than an error —
 * the file is still indexed and usable.
 */
export function CoverThumb({ photoId, alt }: { photoId: number; alt: string }) {
  const [url, setUrl] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setUrl(null);
    setFailed(false);
    api
      .getThumbnail(photoId, "grid")
      .then((t) => {
        if (!cancelled) setUrl(t.data_url);
      })
      .catch(() => {
        if (!cancelled) setFailed(true);
      });
    return () => {
      cancelled = true;
    };
  }, [photoId]);

  if (failed || (url === null)) {
    return <span className="cover-thumb cover-thumb-muted" aria-hidden />;
  }
  return (
    <span className="cover-thumb" title={alt}>
      <img src={url} alt={alt} />
    </span>
  );
}
