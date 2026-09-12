import { useId } from "react";
import type { PhotoHistogram as PhotoHistogramData } from "@/types/api";

export type HistogramMode = "luma" | "rgb";

const WIDTH = 256;
const HEIGHT = 80;

/** Build an unsmoothed, linear-scale area path for 256 measured bins. */
export function histogramAreaPath(values: number[], maximum: number): string {
  const safeMaximum = Math.max(0, maximum);
  const points = Array.from({ length: 256 }, (_, index) => {
    const count = Math.max(0, values[index] ?? 0);
    const x = index * (WIDTH / 255);
    const y = safeMaximum === 0 ? HEIGHT : HEIGHT - (count / safeMaximum) * HEIGHT;
    return `${x.toFixed(2)} ${y.toFixed(2)}`;
  });
  return `M 0 ${HEIGHT} L ${points.join(" L ")} L ${WIDTH} ${HEIGHT} Z`;
}

function maxBin(...channels: number[][]): number {
  return channels.reduce(
    (maximum, channel) => channel.reduce((inner, count) => Math.max(inner, count), maximum),
    0,
  );
}

/** A factual distribution of the exact local JPEG preview shown beside it. */
export function PhotoHistogram({ histogram, mode, onModeChange, className = "" }: {
  histogram: PhotoHistogramData;
  mode: HistogramMode;
  onModeChange: (mode: HistogramMode) => void;
  className?: string;
}) {
  const titleId = useId();
  const lumaMaximum = maxBin(histogram.luma);
  const rgbMaximum = maxBin(histogram.red, histogram.green, histogram.blue);

  return (
    <section className={`photo-histogram ${className}`.trim()} aria-labelledby={titleId}>
      <div className="photo-histogram-head">
        <h4 id={titleId}>Histogram</h4>
        <div className="photo-histogram-modes" role="group" aria-label="Histogram channels">
          <button type="button" aria-pressed={mode === "luma"} onClick={() => onModeChange("luma")}>Luma</button>
          <button type="button" aria-pressed={mode === "rgb"} onClick={() => onModeChange("rgb")}>RGB</button>
        </div>
      </div>
      <svg
        className="photo-histogram-chart"
        viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
        preserveAspectRatio="none"
        role="img"
        aria-label={`${mode === "luma" ? "Luminance" : "Red, green, and blue"} distribution of the rendered preview`}
      >
        {mode === "luma" ? (
          <path className="photo-histogram-luma" d={histogramAreaPath(histogram.luma, lumaMaximum)} />
        ) : (
          <>
            <path className="photo-histogram-red" d={histogramAreaPath(histogram.red, rgbMaximum)} />
            <path className="photo-histogram-green" d={histogramAreaPath(histogram.green, rgbMaximum)} />
            <path className="photo-histogram-blue" d={histogramAreaPath(histogram.blue, rgbMaximum)} />
          </>
        )}
      </svg>
      <div className="photo-histogram-scale mono" aria-hidden="true">
        <span>0</span>
        <span>Rendered preview</span>
        <span>255</span>
      </div>
    </section>
  );
}
