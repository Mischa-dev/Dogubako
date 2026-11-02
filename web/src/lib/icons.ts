import {
  AudioLines, Clapperboard, CodeXml, Cpu, FileText, FolderArchive, Globe, Image, Mic, Palette, Sheet, Type, Wrench,
  type LucideIcon,
} from 'lucide-react';

export const CATEGORY_ICONS: Record<string, LucideIcon> = {
  image: Image,
  video: Clapperboard,
  audio: AudioLines,
  speech: Mic,
  pdf: FileText,
  files: FolderArchive,
  text: Type,
  dev: CodeXml,
  data: Sheet,
  network: Globe,
  design: Palette,
  system: Cpu,
};

export const categoryIcon = (id: string): LucideIcon => CATEGORY_ICONS[id] ?? Wrench;

export const CATEGORY_ORDER = ['image', 'video', 'audio', 'speech', 'pdf', 'files', 'text', 'dev', 'data', 'network', 'design', 'system'];

export const CATEGORY_LABELS: Record<string, string> = {
  image: 'Images', video: 'Video', audio: 'Audio', speech: 'Speech & Voice', pdf: 'PDF & Documents', files: 'Files',
  text: 'Text', dev: 'Developer', data: 'Data', network: 'Network & Web', design: 'Design', system: 'System',
};

export const CATEGORY_DESCRIPTIONS: Record<string, string> = {
  image: 'Convert, resize, compress, inspect and edit images',
  video: 'Convert, compress, trim and inspect video',
  audio: 'Convert, trim, normalize and analyze audio',
  speech: 'Offline transcription, voice activity and subtitles',
  pdf: 'Merge, split, OCR, protect and convert documents',
  files: 'Checksums, archives, duplicates and file inspection',
  text: 'Transform, compare and clean up text',
  dev: 'Formatters, encoders, hashes and generators',
  data: 'CSV, spreadsheets and structured data',
  network: 'DNS, HTTP, TLS, IP and URL utilities',
  design: 'Colors, palettes, gradients and icons',
  system: 'Hardware, battery, storage and OS information',
};
