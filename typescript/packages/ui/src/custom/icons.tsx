import type React from 'react';
import {
  SiClerk,
  SiDiscord,
  SiGithub,
  SiOpenai,
  SiOpenaiHex,
  SiSlack,
  SiStripe,
  SiX,
} from '@icons-pack/react-simple-icons';
import type { VariantProps } from 'class-variance-authority';
import { cva } from 'class-variance-authority';
import type { IconNode, LucideProps } from 'lucide-react';
import {
  AlertCircle,
  AlertTriangle,
  ArrowBigUp,
  ArrowDownToLine,
  ArrowLeft,
  ArrowRight,
  ArrowUp,
  ArrowUpDown,
  ArrowUpFromLine,
  BadgeCheck,
  Ban,
  BarChart2,
  Bookmark,
  BookmarkPlus,
  Calendar,
  ChartNoAxesColumn,
  Check,
  CheckCheck,
  CheckCircle2,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  ChevronUp,
  ChevronsLeft,
  ChevronsLeftRightEllipsis,
  ChevronsRight,
  Circle,
  CircleDot,
  CirclePlus,
  CircleStop,
  Clock,
  Command,
  Copy,
  CornerDownLeft,
  Delete,
  DollarSign,
  Dot,
  Download,
  ExternalLink,
  Eye,
  Flame,
  FlaskConical,
  FunctionSquare,
  GalleryVerticalEnd,
  Heart,
  Home,
  Info,
  Key,
  LayoutGrid,
  ListFilter,
  ListOrdered,
  ListPlus,
  Loader2,
  LogIn,
  Mail,
  MapPin,
  Maximize,
  Menu,
  MessageCircleQuestion,
  MessageSquareText,
  Mic,
  Moon,
  MoreVertical,
  Option,
  PanelLeft,
  Paperclip,
  PartyPopper,
  Pencil,
  PieChart,
  Play,
  Plus,
  Rocket,
  Search,
  Settings,
  Share,
  SlidersHorizontal,
  Sparkles,
  SquarePen,
  Star,
  SunMedium,
  Text,
  ThumbsDown,
  ThumbsUp,
  Trash,
  Triangle,
  Upload,
  User,
  UsersRound,
  X,
} from 'lucide-react';
import type { TwcComponentProps } from 'react-twc';

import { cn, twx } from '@baml/ui/lib/utils';

export type Icon = IconNode;

export const iconVariants = cva('shrink-0', {
  defaultVariants: {
    size: 'default',
  },
  variants: {
    size: {
      '2xl': 'size-8',
      default: 'size-5',
      lg: 'size-6',
      sm: 'size-4',
      xl: 'size-7',
      xs: 'size-3',
    },

    variant: {
      destructive: 'text-destructive',
      loading: 'text-muted',
      muted: 'text-muted-foreground',
      primary: 'text-primary',
      'primary-darker': 'text-primary-darker',
      secondary: 'text-secondary',
      warning: 'text-warning',
    },
  },
});

export type IconProps = TwcComponentProps<'svg'> &
  LucideProps &
  VariantProps<typeof iconVariants>;

export type SiIconProps = TwcComponentProps<'svg'> &
  VariantProps<typeof iconVariants> & { withColor?: boolean };

export const Icons = {
  Mic: twx(Mic).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Clerk: twx(SiClerk).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Stripe: twx(SiStripe).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Slack: twx(SiSlack).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Github: twx(SiGithub).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  LogIn: twx(LogIn).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Key: twx(Key).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Rocket: twx(Rocket).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  LayoutGrid: twx(LayoutGrid).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Clock: twx(Clock).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  ArrowUpDown: twx(ArrowUpDown).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  ChevronsLeftRightEllipsis: twx(ChevronsLeftRightEllipsis).transientProps([
    'size',
    'variant',
  ])<IconProps>(({ size, variant }) => iconVariants({ size, variant })),
  Trash: twx(Trash).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  AlertCircle: twx(AlertCircle).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  AlertTriangle: twx(AlertTriangle).transientProps([
    'size',
    'variant',
  ])<IconProps>(({ size, variant }) => iconVariants({ size, variant })),
  ArrowUpFromLine: twx(ArrowUpFromLine).transientProps([
    'size',
    'variant',
  ])<IconProps>(({ size, variant }) => iconVariants({ size, variant })),
  ArrowDownToLine: twx(ArrowDownToLine).transientProps([
    'size',
    'variant',
  ])<IconProps>(({ size, variant }) => iconVariants({ size, variant })),
  FunctionSquare: twx(FunctionSquare).transientProps([
    'size',
    'variant',
  ])<IconProps>(({ size, variant }) => iconVariants({ size, variant })),
  FlaskConical: twx(FlaskConical).transientProps([
    'size',
    'variant',
  ])<IconProps>(({ size, variant }) => iconVariants({ size, variant })),

  AngelList: ({ size, className }: IconProps) => (
    <svg
      className={cn(iconVariants({ size }), className)}
      viewBox="0 0 256 369"
      xmlns="http://www.w3.org/2000/svg"
    >
      <title className="sr-only">AngelList</title>
      <path d="M214.9 157.358c15.265 2.348 25.834 9.394 31.706 18.789 5.871 9.394 9.394 25.835 9.394 46.972 0 42.275-12.917 77.505-38.752 104.514-25.835 27.01-58.716 41.1-98.642 41.1-15.267 0-30.533-2.348-45.799-8.22-15.266-5.87-27.009-14.091-37.578-23.485-11.743-10.57-21.137-23.487-27.009-35.23C3.523 288.881 0 275.963 0 263.046c0-14.092 3.523-25.835 9.394-34.055 5.872-8.22 16.44-12.918 29.358-16.44-2.348-5.872-4.697-10.57-5.871-15.267-1.175-3.523-2.349-7.045-2.349-9.394 0-7.046 3.523-15.266 11.743-23.486s15.266-11.743 22.312-11.743c3.523 0 5.872 0 9.395 1.174 3.523 1.174 7.046 2.348 11.743 5.871-10.569-35.229-21.138-64.587-27.01-84.55-5.871-19.963-8.22-32.88-8.22-41.101 0-10.569 2.349-18.789 8.22-24.66C63.414 3.522 71.634 0 81.029 0c15.266 0 35.229 35.23 59.89 106.862 4.697 11.744 7.045 21.138 9.394 28.184 2.349-4.697 4.697-12.918 8.22-22.312C183.192 42.275 204.33 7.046 220.771 7.046c8.22 0 15.266 2.348 21.137 8.22 4.698 5.872 8.22 14.092 8.22 23.486 0 7.046-2.348 21.138-8.22 41.101-7.046 19.964-15.266 45.798-27.009 77.505zM34.054 260.698c2.349 2.348 5.872 7.045 9.395 12.917C54.018 288.88 64.587 297.1 73.982 297.1c3.523 0 5.871-1.174 8.22-3.523 2.348-2.349 3.523-4.697 3.523-5.872 0-2.348-1.175-7.045-4.697-12.917-3.523-5.872-8.22-12.917-14.092-19.963-7.046-8.22-12.918-15.266-16.44-18.79-4.698-3.522-8.22-5.87-10.57-5.87-5.871 0-11.743 3.522-16.44 9.394-4.697 5.871-7.046 14.091-7.046 22.312 0 7.045 1.175 14.091 4.698 23.486 3.523 8.22 8.22 17.614 15.266 25.835 10.568 11.743 22.312 22.312 37.578 29.357 15.266 7.046 30.532 10.57 49.32 10.57 32.881 0 59.89-11.744 82.203-36.404 22.312-24.661 32.88-55.193 32.88-92.771 0-11.743-1.174-19.963-2.348-27.01-1.175-7.045-4.698-11.742-8.22-14.091-7.046-5.872-19.964-10.569-39.927-15.266-19.963-4.697-41.101-7.046-62.239-7.046-5.871 0-10.568 1.174-12.917 3.523s-3.523 5.872-3.523 10.569c0 11.743 5.872 19.963 18.789 24.66 12.917 4.698 34.055 8.22 62.239 8.22h10.568c2.349 0 4.698 1.175 5.872 2.35 1.174 2.348 2.349 4.696 2.349 8.22-2.35 2.348-8.22 5.871-17.615 9.394-9.395 3.523-15.266 7.046-19.963 10.569-10.57 7.046-18.79 16.44-24.661 27.009-5.872 10.569-9.395 19.963-9.395 29.358 0 5.871 1.175 11.743 3.523 19.963 2.35 8.22 3.523 12.917 3.523 14.092v4.697c-7.046 0-12.917-4.697-17.614-12.917-4.698-8.22-5.872-18.79-5.872-32.881v-2.349c-1.174 1.174-2.348 2.349-3.523 2.349-1.174 0-2.348 1.174-4.697 1.174h-4.697c-1.175 0-2.349-1.174-4.698-1.174 0 2.348 1.175 3.523 1.175 5.871v4.698c0 5.871-2.349 11.743-7.046 16.44-4.697 4.697-10.569 7.046-17.615 7.046-10.569 0-21.137-4.697-32.88-15.266-10.57-10.569-16.44-19.964-16.44-30.532 0-2.349 0-3.523 1.173-4.698 0-5.871 1.175-7.045 2.349-8.22zm76.33 5.87c2.349 0 5.872-1.174 8.22-3.522 2.35-2.349 3.523-5.872 3.523-8.22 0-3.523-2.348-10.57-7.045-22.312-4.698-11.743-10.57-23.486-17.615-34.055-4.697-8.22-10.569-15.266-15.266-18.79-4.697-4.697-9.395-5.87-14.092-5.87-3.523 0-7.046 2.348-11.743 7.045s-5.872 8.22-5.872 12.917c0 3.523 2.349 10.57 5.872 18.79 4.697 8.22 9.394 16.44 16.44 25.834C79.853 247.78 86.9 256 93.945 261.872c7.046 2.348 11.743 4.697 16.44 4.697zm24.66-122.128l-28.183-78.679c-7.045-19.963-11.743-34.055-16.44-39.926-3.523-5.872-7.046-9.395-11.743-9.395-3.523 0-5.872 1.175-8.22 3.523-3.523 3.523-4.698 7.046-4.698 11.743 0 8.22 3.523 21.138 9.395 39.927 5.872 18.789 15.266 44.624 27.01 75.156 1.173-2.349 2.348-3.523 4.696-3.523 2.349-1.174 4.698-1.174 7.046-1.174h5.872c3.523 1.174 8.22 1.174 15.266 2.348zm28.184 76.33c-7.046 0-14.091-1.174-21.137-2.348-7.046-1.174-12.918-2.349-18.79-4.697 2.35 5.871 4.698 10.569 7.047 16.44 2.348 5.872 3.523 10.569 4.697 16.44 3.523-4.697 8.22-9.394 12.917-14.091 5.872-4.697 10.57-8.22 15.266-11.743zm34.055-68.11c11.744-30.532 19.964-56.366 27.01-76.33 5.871-19.963 9.394-31.706 9.394-36.403 0-4.698-1.174-8.22-3.523-11.744-2.348-2.348-4.697-3.522-8.22-3.522-4.697 0-9.395 3.522-14.092 11.743-4.697 8.22-10.569 19.963-16.44 37.578l-25.835 72.807 31.706 5.872z" />
    </svg>
  ),
  ArrowBigUp: twx(ArrowBigUp).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  ArrowLeft: twx(ArrowLeft).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  ArrowRight: twx(ArrowRight).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  ArrowUp: twx(ArrowUp).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  BadgeCheck: twx(BadgeCheck).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Ban: twx(Ban).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  BarChart2: twx(BarChart2).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Bookmark: twx(Bookmark).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  BookmarkPlus: twx(BookmarkPlus).transientProps([
    'size',
    'variant',
  ])<IconProps>(({ size, variant }) => iconVariants({ size, variant })),
  Calendar: twx(Calendar).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  ChartNoAxesColumn: twx(ChartNoAxesColumn).transientProps([
    'size',
    'variant',
  ])<IconProps>(({ size, variant }) => iconVariants({ size, variant })),
  Check: twx(Check).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  CheckCheck: twx(CheckCheck).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  CheckCircle2: twx(CheckCircle2).transientProps([
    'size',
    'variant',
  ])<IconProps>(({ size, variant }) => iconVariants({ size, variant })),
  ChevronDown: twx(ChevronDown).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  ChevronLeft: twx(ChevronLeft).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  ChevronRight: twx(ChevronRight).transientProps([
    'size',
    'variant',
  ])<IconProps>(({ size, variant }) => iconVariants({ size, variant })),
  ChevronUp: twx(ChevronUp).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  ChevronsLeft: twx(ChevronsLeft).transientProps([
    'size',
    'variant',
  ])<IconProps>(({ size, variant }) => iconVariants({ size, variant })),
  ChevronsRight: twx(ChevronsRight).transientProps([
    'size',
    'variant',
  ])<IconProps>(({ size, variant }) => iconVariants({ size, variant })),
  Circle: twx(Circle).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  CircleDot: twx(CircleDot).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  CirclePlus: twx(CirclePlus).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  CircleStop: twx(CircleStop).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Command: twx(Command).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Copy: twx(Copy).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  CornerDownLeft: twx(CornerDownLeft).transientProps([
    'size',
    'variant',
  ])<IconProps>(({ size, variant }) => iconVariants({ size, variant })),
  CrunchBase: ({ size, className }: IconProps) => (
    <svg
      className={cn(iconVariants({ size }), className)}
      viewBox="0 0 24 24"
      xmlns="http://www.w3.org/2000/svg"
    >
      <title className="sr-only">CrunchBase</title>
      <path d="M21.6 0H2.4A2.41 2.41 0 0 0 0 2.4v19.2A2.41 2.41 0 0 0 2.4 24h19.2a2.41 2.41 0 0 0 2.4-2.4V2.4A2.41 2.41 0 0 0 21.6 0zM7.045 14.465A2.11 2.11 0 0 0 9.84 13.42h1.66a3.69 3.69 0 1 1 0-1.75H9.84a2.11 2.11 0 1 0-2.795 2.795zm11.345.845a3.55 3.55 0 0 1-1.06.63 3.68 3.68 0 0 1-3.39-.38v.38h-1.51V5.37h1.5v4.11a3.74 3.74 0 0 1 1.8-.63H16a3.67 3.67 0 0 1 2.39 6.46zm-.223-2.766a2.104 2.104 0 1 1-4.207 0 2.104 2.104 0 0 1 4.207 0z" />
    </svg>
  ),
  Discord: twx(SiDiscord).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Delete: twx(Delete).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  DollarSign: twx(DollarSign).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Dot: twx(Dot).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Download: twx(Download).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  ExternalLink: twx(ExternalLink).transientProps([
    'size',
    'variant',
  ])<IconProps>(({ size, variant }) => iconVariants({ size, variant })),
  Eye: twx(Eye).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Flame: twx(Flame).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  GalleryVerticalEnd: twx(GalleryVerticalEnd).transientProps([
    'size',
    'variant',
  ])<IconProps>(({ size, variant }) => iconVariants({ size, variant })),
  Google: ({ size, className }: IconProps) => (
    <svg
      className={cn(iconVariants({ size }), className)}
      viewBox="0 0 24 24"
      xmlns="http://www.w3.org/2000/svg"
    >
      <title className="sr-only">Google</title>
      <path
        d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z"
        fill="#4285F4"
      />
      <path
        d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z"
        fill="#34A853"
      />
      <path
        d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z"
        fill="#FBBC05"
      />
      <path
        d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z"
        fill="#EA4335"
      />
      <path d="M1 1h22v22H1z" fill="none" />
    </svg>
  ),
  GoogleDocs: (props: SiIconProps) => (
    <SiOpenai
      className={iconVariants({ size: props.size })}
      color={props.withColor ? SiOpenaiHex : undefined}
    />
  ),
  Heart: twx(Heart).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Home: twx(Home).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Info: twx(Info).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  ListFilter: twx(ListFilter).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  ListOrdered: twx(ListOrdered).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  ListPlus: twx(ListPlus).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Mail: twx(Mail).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  MapPin: twx(MapPin).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Maximize: twx(Maximize).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Menu: twx(Menu).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  MessageCircleQuestion: twx(MessageCircleQuestion).transientProps([
    'size',
    'variant',
  ])<IconProps>(({ size, variant }) => iconVariants({ size, variant })),
  MessageSquareText: twx(MessageSquareText).transientProps([
    'size',
    'variant',
  ])<IconProps>(({ size, variant }) => iconVariants({ size, variant })),
  Moon: twx(Moon).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  MoreVertical: twx(MoreVertical).transientProps([
    'size',
    'variant',
  ])<IconProps>(({ size, variant }) => iconVariants({ size, variant })),
  Option: twx(Option).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  PanelLeft: twx(PanelLeft).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Paperclip: twx(Paperclip).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  PartyPopper: twx(PartyPopper).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Pencil: twx(Pencil).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  PieChart: twx(PieChart).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Play: twx(Play).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Plus: twx(Plus).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Search: twx(Search).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Settings: twx(Settings).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Share: twx(Share).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  SlidersHorizontal: twx(SlidersHorizontal).transientProps([
    'size',
    'variant',
  ])<IconProps>(({ size, variant }) => iconVariants({ size, variant })),
  Sparkles: twx(Sparkles).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Spinner: twx(Loader2).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) =>
      cn(iconVariants({ size, variant }), 'origin-center animate-spin'),
  ),
  SquarePen: twx(SquarePen).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Star: twx(Star).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  SunMedium,
  Text: twx(Text).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  User: twx(User).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  ThumbsDown: twx(ThumbsDown).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  ThumbsUp: twx(ThumbsUp).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Triangle: twx(Triangle).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  TwitterX: twx(SiX).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  Upload: twx(Upload).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  UsersRound: twx(UsersRound).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
  X: twx(X).transientProps(['size', 'variant'])<IconProps>(
    ({ size, variant }) => iconVariants({ size, variant }),
  ),
};
