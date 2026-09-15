import { ImageResponse } from "next/og";

export const alt = "AgentTx — Transactions for AI agents";
export const size = { width: 1200, height: 630 };
export const contentType = "image/png";

export default function OpengraphImage() {
  return new ImageResponse(
    (
      <div
        style={{
          width: "100%",
          height: "100%",
          display: "flex",
          flexDirection: "column",
          justifyContent: "space-between",
          padding: 72,
          background: "radial-gradient(900px 480px at 50% -10%, rgba(57,135,229,0.28), #070809 70%)",
          color: "#f5f5f4",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 18, fontSize: 38, fontWeight: 600 }}>
          <div
            style={{
              width: 56,
              height: 56,
              borderRadius: 16,
              background: "#f5f5f4",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
            }}
          >
            <div style={{ width: 22, height: 22, borderRadius: 11, background: "#3987e5" }} />
          </div>
          AgentTx
        </div>
        <div style={{ display: "flex", flexDirection: "column" }}>
          <div style={{ fontSize: 88, fontWeight: 700, letterSpacing: -3, lineHeight: 1.02 }}>Transactions for AI agents.</div>
          <div style={{ marginTop: 28, fontSize: 32, color: "#b4b3ad" }}>
            Root-cause rollbacks · millisecond rewinds · Saga compensation · Clean Hints
          </div>
        </div>
        <div style={{ display: "flex", fontSize: 26, color: "#8c8b85" }}>Open source · Rust · gRPC · RocksDB</div>
      </div>
    ),
    size,
  );
}
