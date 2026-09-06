export function Architecture() {
  return (
    <figure className="figure architecture">
      <svg
        viewBox="0 0 920 360"
        role="img"
        aria-label="Control-plane architecture: workbench and Python evaluation talk to micp-api, which calls policy, SLO, router, optimizer, and simulation crates over micp-core."
      >
        <rect x="8" y="8" width="904" height="344" fill="none" stroke="currentColor" strokeWidth="0.6" />
        <text x="24" y="32" className="svg-kicker">
          FIG. 1  ·  DECISION LOOP
        </text>

        <Box x={28} y={52} w={260} h={70} title="Workbench  ·  TypeScript / React" sub="scenario · constraints · charts" />
        <Box x={632} y={52} w={260} h={70} title="micp_eval  ·  Python" sub="sweeps · sensitivity · artifacts" />

        <path d="M158 122 V148 H460" className="svg-flow" />
        <path d="M762 122 V148 H460" className="svg-flow" />

        <Box x={300} y={148} w={320} h={64} title="micp-api  ·  Axum" sub="/v1/evaluate  /v1/simulate  /v1/scenarios" accent />

        <path d="M460 212 V236" className="svg-flow" />

        <Box x={28} y={236} w={160} h={56} title="policy" sub="degrade, never relax" />
        <Box x={204} y={236} w={160} h={56} title="slo" sub="error budget" />
        <Box x={380} y={236} w={160} h={56} title="router" sub="plan + Pareto" />
        <Box x={556} y={236} w={160} h={56} title="optimizer" sub="weights + ties" />
        <Box x={732} y={236} w={160} h={56} title="sim" sub="seeded G/G/n" />

        <path d="M108 292 V314 H812 V292" className="svg-flow" />
        <Box x={300} y={302} w={320} h={40} title="micp-core  ·  units, domain, estimate" sub="" compact />
      </svg>
      <figcaption>
        The TypeScript workbench does not reimplement routing. Modeled and simulated
        numbers come from the Rust engine over HTTP. Python remains off the request
        path. WASM exposes only the compact inventory score and closed-form p99 helpers.
      </figcaption>
    </figure>
  );
}

function Box({
  x,
  y,
  w,
  h,
  title,
  sub,
  accent,
  compact,
}: {
  x: number;
  y: number;
  w: number;
  h: number;
  title: string;
  sub: string;
  accent?: boolean;
  compact?: boolean;
}) {
  return (
    <g transform={`translate(${x} ${y})`}>
      <rect
        width={w}
        height={h}
        fill={accent ? "var(--ink)" : "var(--paper-2)"}
        stroke="var(--ink)"
        strokeWidth={accent ? 0 : 0.8}
      />
      <text
        x={12}
        y={compact ? 24 : 24}
        fill={accent ? "var(--paper)" : "var(--ink)"}
        fontFamily="var(--sans)"
        fontSize={compact ? 12 : 13}
        fontWeight={600}
      >
        {title}
      </text>
      {sub ? (
        <text
          x={12}
          y={44}
          fill={accent ? "var(--paper-2)" : "var(--muted)"}
          fontFamily="var(--mono)"
          fontSize={10}
        >
          {sub}
        </text>
      ) : null}
    </g>
  );
}
