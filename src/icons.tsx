import type { ReactNode, SVGProps } from "react";
import * as HeroIcons from "@heroicons/react/24/outline";

type IconProps = SVGProps<SVGSVGElement> & {
  size?: number;
  strokeWidth?: number;
};

type HeroIcon = (props: SVGProps<SVGSVGElement> & { title?: string; titleId?: string }) => ReactNode;

function hero(Icon: HeroIcon) {
  return function AppIcon({ size = 16, strokeWidth: _strokeWidth, className, style, ...props }: IconProps) {
    return (
      <Icon
        aria-hidden="true"
        className={className}
        height={size}
        style={style}
        width={size}
        {...props}
      />
    );
  };
}

export type AppIcon = ReturnType<typeof hero>;

export const ArrowDownToLine = hero(HeroIcons.ArrowDownTrayIcon);
export const ArrowTopRight = hero(HeroIcons.ArrowTopRightOnSquareIcon);
export const ArrowUpFromLine = hero(HeroIcons.ArrowDownTrayIcon);
export const BarChart3 = hero(HeroIcons.ChartBarIcon);
export const Book = hero(HeroIcons.BookOpenIcon);
export const Bot = hero(HeroIcons.SparklesIcon);
export const Box = hero(HeroIcons.CubeIcon);
export const Boxes = hero(HeroIcons.Square2StackIcon);
export const BrainCog = hero(HeroIcons.Cog6ToothIcon);
export const Bug = hero(HeroIcons.BugAntIcon);
export const CheckCircle2 = hero(HeroIcons.CheckCircleIcon);
export const ChevronDown = hero(HeroIcons.ChevronDownIcon);
export const ChevronRight = hero(HeroIcons.ChevronRightIcon);
export const Code2 = hero(HeroIcons.CodeBracketIcon);
export const Columns2 = hero(HeroIcons.ViewColumnsIcon);
export const Copy = hero(HeroIcons.ClipboardDocumentIcon);
export const Cpu = hero(HeroIcons.CpuChipIcon);
export const Download = hero(HeroIcons.ArrowDownTrayIcon);
export const FileArchive = hero(HeroIcons.DocumentArrowDownIcon);
export const FileCode2 = hero(HeroIcons.CodeBracketSquareIcon);
export const FileText = hero(HeroIcons.DocumentTextIcon);
export const Folder = hero(HeroIcons.FolderIcon);
export const FolderOpen = hero(HeroIcons.FolderOpenIcon);
export const FolderPlus = hero(HeroIcons.FolderPlusIcon);
export const Fire = hero(HeroIcons.FireIcon);
export const GitBranch = hero(HeroIcons.ArrowPathRoundedSquareIcon);
export const GitPullRequest = hero(HeroIcons.ArrowPathRoundedSquareIcon);
export const Globe = hero(HeroIcons.GlobeAltIcon);
export const Hammer = hero(HeroIcons.WrenchScrewdriverIcon);
export const Info = hero(HeroIcons.InformationCircleIcon);
export const KeyRound = hero(HeroIcons.KeyIcon);
export const Laptop = hero(HeroIcons.ComputerDesktopIcon);
export const ListChecks = hero(HeroIcons.ListBulletIcon);
export const ListTree = hero(HeroIcons.ListBulletIcon);
export const Maximize2 = hero(HeroIcons.ArrowsPointingOutIcon);
export const MemoryStick = hero(HeroIcons.ServerStackIcon);
export const Minus = hero(HeroIcons.MinusIcon);
export const MoreHorizontal = hero(HeroIcons.EllipsisHorizontalIcon);
export const Package = hero(HeroIcons.CubeIcon);
export const Palette = hero(HeroIcons.SwatchIcon);
export const PanelBottomClose = hero(HeroIcons.XMarkIcon);
export const PanelLeft = hero(HeroIcons.RectangleGroupIcon);
export const PanelLeftClose = hero(HeroIcons.RectangleGroupIcon);
export const PencilSquare = hero(HeroIcons.PencilSquareIcon);
export const Plus = hero(HeroIcons.PlusIcon);
export const Radio = hero(HeroIcons.RadioIcon);
export const Redo2 = hero(HeroIcons.ArrowUturnRightIcon);
export const RefreshCw = hero(HeroIcons.ArrowPathIcon);
export const RotateCcw = hero(HeroIcons.ArrowPathIcon);
export const Search = hero(HeroIcons.MagnifyingGlassIcon);
export const Server = hero(HeroIcons.ServerIcon);
export const Settings = hero(HeroIcons.Cog6ToothIcon);
export const ShieldCheck = hero(HeroIcons.ShieldCheckIcon);
export const Smile = hero(HeroIcons.FaceSmileIcon);
export const Sparkles = hero(HeroIcons.SparklesIcon);
export const Square = hero(HeroIcons.StopIcon);
export const SquareTerminal = hero(HeroIcons.CommandLineIcon);
export const Star = hero(HeroIcons.StarIcon);
export const TerminalSquare = hero(HeroIcons.CommandLineIcon);
export const Trash = hero(HeroIcons.TrashIcon);
export const Trophy = hero(HeroIcons.TrophyIcon);
export const Undo2 = hero(HeroIcons.ArrowUturnLeftIcon);
export const Users = hero(HeroIcons.UserGroupIcon);
export const Window = hero(HeroIcons.WindowIcon);
export const Wrench = hero(HeroIcons.WrenchIcon);
export const X = hero(HeroIcons.XMarkIcon);
export const Zap = hero(HeroIcons.BoltIcon);
