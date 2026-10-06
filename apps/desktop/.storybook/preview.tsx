import type { Preview } from "@storybook/react-vite";
import "../src/theme";

const preview: Preview = {
  parameters: {
    controls: {
      matchers: {
        color: /(background|color)$/i,
        date: /Date$/i,
      },
    },
    layout: "centered",
  },
  decorators: [
    (Story) => {
      return (
        <div style={{ minHeight: "100vh", padding: 24, background: "var(--bg)" }}>
          <Story />
        </div>
      );
    },
  ],
};

export default preview;
