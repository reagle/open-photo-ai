import { fireEvent, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import "@/i18n";
import { isMacOs, isWindows } from "@/ipc/os";
import { useRecentStore } from "@/stores/recent";
import { render } from "@/test/support";
import { OpenRecentMenu } from "./OpenRecentMenu";

vi.mock("@/ipc/os", () => ({ isMacOs: vi.fn(() => false), isWindows: vi.fn(() => false) }));
// What choosing does is `lib/recent.test.ts`'s; here, that the button reaches it.
vi.mock("@/lib/recent", () => ({ openRecent: vi.fn(() => Promise.resolve()) }));
vi.mock("@/ipc/recent", () => ({ clearRecent: vi.fn(() => Promise.resolve()) }));

const { openRecent } = await import("@/lib/recent");
const { clearRecent } = await import("@/ipc/recent");

const button = () => screen.queryByRole("button", { name: "Open recent" });

/** Opens the menu through the keyboard: jsdom dispatches no `pointerdown`, which Radix's trigger opens on. */
const open = () => fireEvent.keyDown(screen.getByRole("button", { name: "Open recent" }), { key: "Enter" });

beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(isMacOs).mockReturnValue(false);
    useRecentStore.setState({ paths: ["/trips/rome/a.jpg", "/home/b.png"] });
});

describe.each([
    ["Windows", true],
    ["Linux", false],
])("OpenRecentMenu on %s", (_platform, windows) => {
    beforeEach(() => {
        vi.mocked(isWindows).mockReturnValue(windows);
    });

    it("lists the recent photographs by name, newest first, then Clear Menu", async () => {
        render(<OpenRecentMenu />);

        open();

        const items = await screen.findAllByRole("menuitem");
        expect(items.map((item) => item.textContent)).toEqual(["a.jpg", "b.png", "Clear Menu"]);
        expect(items[0]).toHaveAttribute("title", "/trips/rome/a.jpg");
    });

    it("opens the one chosen", async () => {
        render(<OpenRecentMenu />);

        open();
        fireEvent.click(await screen.findByRole("menuitem", { name: "b.png" }));

        expect(openRecent).toHaveBeenCalledExactlyOnceWith("/home/b.png");
    });

    it("clears the list", async () => {
        render(<OpenRecentMenu />);

        open();
        fireEvent.click(await screen.findByRole("menuitem", { name: "Clear Menu" }));

        expect(clearRecent).toHaveBeenCalledTimes(1);
    });

    it("is unavailable while the list is empty", () => {
        useRecentStore.setState({ paths: [] });
        render(<OpenRecentMenu />);

        expect(button()).toBeDisabled();
    });
});

describe("OpenRecentMenu on macOS", () => {
    it("is not drawn: the menu bar has File > Open Recent", () => {
        vi.mocked(isMacOs).mockReturnValue(true);
        render(<OpenRecentMenu />);

        expect(button()).toBeNull();
    });
});
