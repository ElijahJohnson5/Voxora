import { Moon, Sun, Monitor } from "lucide-react";
import { useTheme } from "@/lib/theme";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

const themeOptions = [
  { value: "light" as const, icon: Sun, label: "Light" },
  { value: "dark" as const, icon: Moon, label: "Dark" },
  { value: "system" as const, icon: Monitor, label: "System" },
];

export function Header() {
  const { theme, setTheme } = useTheme();

  const current =
    themeOptions.find((o) => o.value === theme) ?? themeOptions[2];
  const Icon = current.icon;

  return (
    <div className="flex h-12 shrink-0 items-center justify-between border-b border-border px-4">
      <div className="flex items-center gap-2">
        <span className="text-muted-foreground">#</span>
        <h1 className="text-sm font-semibold">general</h1>
        <span className="mx-2 h-4 w-px bg-border" />
        <span className="text-xs text-muted-foreground">
          General discussion and announcements
        </span>
      </div>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button className="flex items-center gap-1.5 rounded-md px-2 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground">
            <Icon className="size-4" />
            <span className="hidden sm:inline">{current.label}</span>
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuRadioGroup
            value={theme}
            onValueChange={(v) =>
              setTheme(v as "light" | "dark" | "system")
            }
          >
            {themeOptions.map((option) => (
              <DropdownMenuRadioItem key={option.value} value={option.value}>
                <option.icon className="size-4" />
                {option.label}
              </DropdownMenuRadioItem>
            ))}
          </DropdownMenuRadioGroup>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
