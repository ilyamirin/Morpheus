module.exports = {
  content: [
    "./crates/morpheus-server/ui/**/*.html",
    "./crates/morpheus-server/ui/assets/**/*.js"
  ],
  theme: {
    extend: {
      colors: {
        morpheus: {
          ink: "#07111f",
          panel: "#0d1726",
          line: "#1d2b3f",
          cloud: "#f6f8fb"
        }
      },
      fontFamily: {
        sans: [
          "Inter",
          "ui-sans-serif",
          "system-ui",
          "-apple-system",
          "BlinkMacSystemFont",
          "Segoe UI",
          "sans-serif"
        ],
        mono: [
          "JetBrains Mono",
          "SFMono-Regular",
          "Consolas",
          "Liberation Mono",
          "monospace"
        ]
      }
    }
  },
  plugins: []
};
