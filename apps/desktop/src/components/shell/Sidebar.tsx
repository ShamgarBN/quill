import {
  BookOpenText,
  Compass,
  Database,
  Users2,
  Lightbulb,
  Library,
  Settings as SettingsIcon,
  ChevronLeft,
  ChevronRight,
} from "lucide-react";
import { useApp, type RouteId } from "@/stores/app";
import { cn } from "@/lib/cn";

interface NavItem {
  id: RouteId;
  label: string;
  icon: typeof BookOpenText;
  hint?: string;
}

const NAV: NavItem[] = [
  { id: "manuscript", label: "Manuscript", icon: BookOpenText },
  { id: "beats", label: "Beat Sheet", icon: Compass },
  { id: "canon", label: "Canon", icon: Database },
  { id: "bible", label: "Character Bible", icon: Users2 },
  { id: "ideas", label: "Idea Park", icon: Lightbulb },
  { id: "research", label: "Research", icon: Library },
];

const FOOTER: NavItem[] = [{ id: "settings", label: "Settings", icon: SettingsIcon }];

export function Sidebar(): JSX.Element {
  const route = useApp((s) => s.route);
  const setRoute = useApp((s) => s.setRoute);
  const collapsed = useApp((s) => s.sidebarCollapsed);
  const toggle = useApp((s) => s.toggleSidebar);
  const project = useApp((s) => s.currentProject);

  return (
    <aside
      className={cn(
        "app-chrome flex shrink-0 flex-col border-r border-line-subtle bg-surface-subtle",
        "transition-[width] duration-200 ease-quill",
        collapsed ? "w-14" : "w-60",
      )}
    >
      <div className="flex items-center justify-between border-b border-line-subtle px-3 py-3">
        {!collapsed && (
          <div className="min-w-0">
            <div className="text-xs font-medium uppercase tracking-wider text-ink-faint">
              Project
            </div>
            <div className="truncate text-sm font-medium text-ink">
              {project?.name ?? "—"}
            </div>
          </div>
        )}
        <button
          type="button"
          className="qbtn-ghost h-7 w-7 p-0"
          onClick={toggle}
          title={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
        >
          {collapsed ? (
            <ChevronRight className="h-4 w-4" />
          ) : (
            <ChevronLeft className="h-4 w-4" />
          )}
        </button>
      </div>

      <nav className="flex flex-1 flex-col gap-0.5 p-2">
        {NAV.map((item) => (
          <NavButton
            key={item.id}
            item={item}
            active={route === item.id}
            collapsed={collapsed}
            onClick={() => setRoute(item.id)}
          />
        ))}
      </nav>

      <div className="border-t border-line-subtle p-2">
        {FOOTER.map((item) => (
          <NavButton
            key={item.id}
            item={item}
            active={route === item.id}
            collapsed={collapsed}
            onClick={() => setRoute(item.id)}
          />
        ))}
      </div>
    </aside>
  );
}

function NavButton({
  item,
  active,
  collapsed,
  onClick,
}: {
  item: NavItem;
  active: boolean;
  collapsed: boolean;
  onClick: () => void;
}): JSX.Element {
  const Icon = item.icon;
  return (
    <button
      type="button"
      onClick={onClick}
      title={collapsed ? item.label : undefined}
      className={cn(
        "group relative flex items-center gap-3 rounded-md px-2 py-1.5 text-sm",
        "transition-colors duration-150 ease-quill",
        active
          ? "bg-accent-subtle text-accent"
          : "text-ink-muted hover:bg-surface-muted hover:text-ink",
      )}
    >
      <Icon className="h-4 w-4 shrink-0" />
      {!collapsed && <span className="truncate">{item.label}</span>}
      {active && !collapsed && (
        <span className="absolute right-2 h-1.5 w-1.5 rounded-full bg-accent" />
      )}
    </button>
  );
}
