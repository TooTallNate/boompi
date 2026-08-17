import { useEffect, useState } from "react";
import { Button } from "@boompi/ui/components/button";
import { SETTINGS_PAGES, SettingsShell } from "@boompi/ui/shell";
import { BoompiContext } from "@boompi/ui/transport";
import { Wrench } from "lucide-react";
import { HardwarePage } from "./hardware";
import { SetupWizard } from "./setup";
import { useBoompi } from "./useBoompi";

function useHashRoute(): string {
  const [hash, setHash] = useState(window.location.hash);
  useEffect(() => {
    const onChange = () => setHash(window.location.hash);
    window.addEventListener("hashchange", onChange);
    return () => window.removeEventListener("hashchange", onChange);
  }, []);
  return hash;
}

export default function App() {
  const conn = useBoompi();
  const route = useHashRoute();

  return (
    <BoompiContext.Provider value={conn}>
      {route === "#/hardware" ? (
        <HardwarePage />
      ) : conn.state?.setup.required ? (
        <SetupWizard currentName={conn.state?.settings.name ?? ""} />
      ) : (
        <SettingsShell
          pages={SETTINGS_PAGES}
          active={route.replace(/^#\//, "") || "general"}
          onNavigate={(id) => {
            window.location.hash = `#/${id}`;
          }}
          footer={
            <Button
              variant="ghost"
              size="sm"
              className="justify-start text-muted-foreground"
              asChild
            >
              <a href="#/hardware" title="Box hardware configuration (advanced)">
                <Wrench data-icon="inline-start" />
                <span className="group-data-[collapsible=icon]:hidden">
                  Box hardware
                </span>
              </a>
            </Button>
          }
        />
      )}
    </BoompiContext.Provider>
  );
}
