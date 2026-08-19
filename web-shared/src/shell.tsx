// The settings shell: shadcn sidebar with one focused page per topic,
// replacing the old endless single-scroll settings column. Apps own
// routing (hash on the box app, in-memory on the remote) and hand the
// shell the active page id.

import type { ComponentType, ReactNode } from "react";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarRail,
  SidebarTrigger,
} from "@boompi/ui/components/sidebar";
import { GithubIcon } from "@boompi/ui/components/github-icon";
import { Separator } from "@boompi/ui/components/separator";
import {
  BatteryMedium,
  Bluetooth,
  CloudDownload,
  Gamepad2,
  Home,
  Monitor,
  Settings2,
  Volume2,
  Wifi,
} from "lucide-react";
import { capsOf } from "@boompi/ui/proto";
import { useBoompi } from "@boompi/ui/transport";

import { AirplaySection } from "@boompi/ui/sections/airplay";
import { AppearanceSection } from "@boompi/ui/sections/appearance";
import { BatterySection } from "@boompi/ui/sections/battery";
import { BluetoothSection } from "@boompi/ui/sections/bluetooth";
import { ClockSection } from "@boompi/ui/sections/clock";
import { EmojiFontsSection } from "@boompi/ui/sections/emoji-fonts";
import { GamesSection } from "@boompi/ui/sections/games";
import { HomeAssistantSection } from "@boompi/ui/sections/home-assistant";
import { ScreensaverSection } from "@boompi/ui/sections/screensaver";
import { SoftwareSection } from "@boompi/ui/sections/software";
import { SpeakerNameSection } from "@boompi/ui/sections/speaker-name";
import { SystemSection } from "@boompi/ui/sections/system";
import { VolumeSection } from "@boompi/ui/sections/volume";
import { WifiSection } from "@boompi/ui/sections/wifi";

export interface SettingsPage {
  id: string;
  label: string;
  icon: ComponentType;
  content: ComponentType;
  /** Capability required for this page to appear (Hello.capabilities;
   *  a hard-wired box without a battery drops its Battery page). */
  requires?: string;
}

function GeneralPage() {
  return (
    <>
      <SpeakerNameSection />
      <ClockSection />
      <SystemSection />
    </>
  );
}

function AudioPage() {
  return (
    <>
      <VolumeSection />
      <AirplaySection />
    </>
  );
}

function DisplayPage() {
  return (
    <>
      <AppearanceSection />
      <ScreensaverSection />
      <EmojiFontsSection />
    </>
  );
}

/** The full page set. Apps can filter (e.g. the remote hides pages that
 *  are useless without an IP path) and append their own. */
export const SETTINGS_PAGES: SettingsPage[] = [
  { id: "general", label: "General", icon: Settings2, content: GeneralPage },
  { id: "audio", label: "Audio & AirPlay", icon: Volume2, content: AudioPage },
  { id: "display", label: "Display", icon: Monitor, content: DisplayPage },
  { id: "bluetooth", label: "Bluetooth", icon: Bluetooth, content: BluetoothSection, requires: "bluetooth" },
  { id: "wifi", label: "Wi-Fi", icon: Wifi, content: WifiSection, requires: "wifi" },
  { id: "games", label: "Games", icon: Gamepad2, content: GamesSection, requires: "games" },
  { id: "battery", label: "Battery", icon: BatteryMedium, content: BatterySection, requires: "battery" },
  { id: "home-assistant", label: "Home Assistant", icon: Home, content: HomeAssistantSection, requires: "home_assistant" },
  { id: "software", label: "Software", icon: CloudDownload, content: SoftwareSection, requires: "updates" },
];

export function SettingsShell({
  pages,
  active,
  onNavigate,
  footer,
  headerExtra,
}: {
  pages: SettingsPage[];
  active: string;
  onNavigate: (id: string) => void;
  /** Extra sidebar footer content (e.g. the hardware-config link). */
  footer?: ReactNode;
  /** Extra header content (e.g. connection status / disconnect). */
  headerExtra?: ReactNode;
}) {
  const { hello, state, error } = useBoompi();
  // Capability gating: the hosted remote and phone apps outlive any
  // box's software, so the box declares what it can do and the UI
  // hides the rest (old boxes get the legacy set).
  const caps = capsOf(hello);
  const shown = pages.filter((p) => !p.requires || caps.has(p.requires));
  const page = shown.find((p) => p.id === active) ?? shown[0];
  const Content = page.content;

  return (
    <SidebarProvider>
      <Sidebar collapsible="icon">
        <SidebarHeader>
          <div className="flex items-center gap-2 px-2 py-1.5 group-data-[collapsible=icon]:px-0">
            <img src="/icon.png" alt="" aria-hidden className="size-7 flex-none" />
            <div className="min-w-0 group-data-[collapsible=icon]:hidden">
              <div className="truncate text-sm font-semibold">
                {state?.settings?.name || "Boompi"}
              </div>
              <div className="truncate text-xs text-muted-foreground">
                {error ??
                  (hello
                    ? // The image stamp already carries its "v"
                      // ("v2.2.0-abc"); dev builds report "dev".
                      `${hello.version} · up ${Math.floor(hello.uptime_secs / 60)} min`
                    : "connecting…")}
              </div>
            </div>
          </div>
        </SidebarHeader>
        <SidebarContent>
          <SidebarGroup>
            <SidebarGroupContent>
              <SidebarMenu>
                {shown.map((p) => (
                  <SidebarMenuItem key={p.id}>
                    <SidebarMenuButton
                      isActive={p.id === page.id}
                      tooltip={p.label}
                      onClick={() => onNavigate(p.id)}
                    >
                      <p.icon />
                      <span>{p.label}</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                ))}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
        </SidebarContent>
        <SidebarFooter>
          {footer}
          <a
            href="https://github.com/TooTallNate/boompi"
            target="_blank"
            rel="noreferrer"
            className="flex items-center gap-2 rounded-md px-2 py-1.5 text-xs text-muted-foreground hover:text-foreground group-data-[collapsible=icon]:justify-center"
            title="Boompi on GitHub"
          >
            <GithubIcon className="size-4 flex-none" />
            <span className="group-data-[collapsible=icon]:hidden">GitHub</span>
          </a>
        </SidebarFooter>
        <SidebarRail />
      </Sidebar>
      <SidebarInset>
        <header className="flex h-14 shrink-0 items-center gap-2 border-b px-4">
          <SidebarTrigger className="-ml-1" />
          <Separator orientation="vertical" className="mr-2 h-4" />
          <h1 className="text-sm font-medium">{page.label}</h1>
          <div className="ml-auto flex items-center gap-2">{headerExtra}</div>
        </header>
        <main className="flex justify-center px-4 py-6">
          <div className="flex w-full max-w-lg flex-col gap-4">
            <Content />
          </div>
        </main>
      </SidebarInset>
    </SidebarProvider>
  );
}
