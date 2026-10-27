<script lang="ts">
  import { formatCredits } from '$lib/format';

  /**
   * One bar per week either side of a zero line: up for a week the carrier
   * gained, down for one it lost. `null` is a week with nothing reported and
   * draws no bar. Hovering a week shows its figure.
   */
  let { labels, values, height = 140 }: { labels: string[]; values: (number | null)[]; height?: number } =
    $props();

  const top = $derived(Math.max(0, ...values.map((v) => v ?? 0)));
  const bottom = $derived(Math.min(0, ...values.map((v) => v ?? 0)));
  const span = $derived(Math.max(1, top - bottom));
  const zero = $derived((top / span) * height);
  const W = 10;
  const GAP = 2;
</script>

<figure class="chart">
  <div class="chart__plot">
    <span class="chart__max">{formatCredits(top)}</span>
    <svg viewBox={`0 0 ${labels.length * W} ${height}`} preserveAspectRatio="none" style:height={`${height}px`} role="img"
      aria-label={labels.map((l, i) => `${l}: ${values[i] === null ? 'no report' : formatCredits(values[i])}`).join(', ')}>
      {#each labels as week, i (week)}
        {@const v = values[i]}
        <g>
          <title>{`${week}\n${v === null ? 'No balance reported' : `${formatCredits(v)} CR`}`}</title>
          <rect x={i * W} y="0" width={W} {height} fill="transparent" />
          {#if v !== null && v !== 0}
            <rect class={v > 0 ? 'chart__fill--full' : 'chart__fill--low'} x={i * W + GAP / 2} width={W - GAP}
              y={v > 0 ? zero - (v / span) * height : zero} height={(Math.abs(v) / span) * height} />
          {/if}
        </g>
      {/each}
      <line class="chart__zero" x1="0" x2={labels.length * W} y1={zero} y2={zero} vector-effect="non-scaling-stroke" />
    </svg>
    {#if bottom < 0}<span class="chart__min">{formatCredits(bottom)}</span>{/if}
  </div>
  <div class="chart__axis">
    <span>{labels[0]}</span>
    <span>{labels[labels.length - 1]}</span>
  </div>
  <figcaption class="chart__legend">
    <span><i class="chart__swatch chart__fill--full"></i>Gained</span>
    <span><i class="chart__swatch chart__fill--low"></i>Lost</span>
  </figcaption>
</figure>
