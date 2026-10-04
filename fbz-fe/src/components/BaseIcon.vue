<script setup lang="ts">
import {
  Play,
  Pause,
  Search,
  House,
  Library,
  Film,
  Tv,
  Settings,
  User,
  LogOut,
  ChevronLeft,
  ChevronRight,
  ChevronDown,
  X,
  Plus,
  RefreshCw,
  Folder,
  Cloud,
  SlidersHorizontal,
  Volume2,
  VolumeX,
  Maximize,
  Minimize,
  Captions,
  SkipForward,
  SkipBack,
  RotateCcw,
  RotateCw,
  LoaderCircle,
  Info,
  Check,
  CheckCircle2,
  AlertCircle,
  Star,
  Clock,
  ArrowUpRight,
  Menu,
  Sun,
  Moon,
  Grid2X2,
  List,
  Heart,
  CircleHelp,
} from "@lucide/vue";
const icons = {
  play: Play,
  pause: Pause,
  search: Search,
  home: House,
  library: Library,
  movie: Film,
  tv: Tv,
  settings: Settings,
  user: User,
  logout: LogOut,
  left: ChevronLeft,
  right: ChevronRight,
  down: ChevronDown,
  close: X,
  plus: Plus,
  refresh: RefreshCw,
  folder: Folder,
  cloud: Cloud,
  filter: SlidersHorizontal,
  volume: Volume2,
  mute: VolumeX,
  fullscreen: Maximize,
  minimize: Minimize,
  subtitles: Captions,
  next: SkipForward,
  previous: SkipBack,
  rewind: RotateCcw,
  forward: RotateCw,
  loading: LoaderCircle,
  info: Info,
  check: Check,
  success: CheckCircle2,
  error: AlertCircle,
  star: Star,
  clock: Clock,
  external: ArrowUpRight,
  menu: Menu,
  sun: Sun,
  moon: Moon,
  grid: Grid2X2,
  list: List,
  heart: Heart,
  help: CircleHelp,
};
const props = withDefaults(defineProps<{ name: keyof typeof icons | string; size?: number }>(), {
  size: 20,
});
</script>
<template>
  <component
    :is="icons[props.name as keyof typeof icons] || Info"
    :size="size"
    :stroke-width="1.7"
    aria-hidden="true"
    class="fbz-icon"
  />
</template>
<style scoped lang="scss">
.fbz-icon {
  flex-shrink: 0;
  vertical-align: middle;
}
</style>
