<script lang="ts" module>
  export interface Series {
    label: string;
    /** A `chart__fill--*` modifier: the shade of the theme colour. */
    fill: 'full' | 'mid' | 'low' | 'text';
    values: number[];
  }
</script>

<script lang="ts">
  import { formatNumber } from '$lib/format';

  /**
   * Stacked daily bars, drawn in the theme's colours. `labels` are the days;
   * each series holds one value per day and stacks in order, first at the
   * bottom. Hovering a bar shows that day's numbers.
   */
  let { labels, series, height = 140 }: { labels: string[]; series: Series[]; height?: number } = $props();

  const totals = $derived(labels.map((_, i) => series.reduce((sum, s) => sum + (s.values[i] ?? 0), 0)));
  const max = $derived(Math.max(1, ...totals));
  // One unit per day, with a gap; the SVG stretches to the width.
  const W = 10;
  const GAP = 2;
</script>

<figure class="chart">
  <div class="chart__plot">
    <span class="chart__max">{formatNumber(max)}</span>
    <svg viewBox={`0 0 ${labels.length * W} ${height}`} preserveAspectRatio="none" style:height={`${height}px`} role="img"
      aria-label={series.map((s) => `${s.label}: ${formatNumber(s.values.reduce((a, b) => a + b, 0))}`).join(', ')}>
      {#each labels as day, i (day)}
        {@const tip = `${day}\n` + series.map((s) => `${s.label}: ${formatNumber(s.values[i] ?? 0)}`).join('\n')}
        <g>
          <title>{tip}</title>
          <!-- Full-height hit area so empty days still show their tooltip. -->
          <rect x={i * W} y="0" width={W} height={height} fill="transparent" />
          {#each series as s, k (s.label)}
            {@const below = series.slice(0, k).reduce((sum, t) => sum + (t.values[i] ?? 0), 0)}
            {@const value = s.values[i] ?? 0}
            {#if value > 0}
              <rect class={`chart__fill--${s.fill}`} x={i * W + GAP / 2} width={W - GAP}
                y={height - ((below + value) / max) * height} height={(value / max) * height} />
            {/if}
          {/each}
        </g>
      {/each}
    </svg>
  </div>
  <div class="chart__axis">
    <span>{labels[0]}</span>
    <span>{labels[labels.length - 1]}</span>
  </div>
  <figcaption class="chart__legend">
    {#each series as s (s.label)}
      <span><i class={`chart__swatch chart__fill--${s.fill}`}></i>{s.label}</span>
    {/each}
  </figcaption>
</figure>
