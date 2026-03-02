import { useTranslation } from "react-i18next";
import { useState, useCallback, useEffect } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

import { Badge } from "@/components/ui/badge";
import { Progress } from "@/components/ui/progress";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { Label } from "@/components/ui/label";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
  DialogDescription,
} from "@/components/ui/dialog";
import { commands } from "@/lib/tauri";
import {
  Mic,
  Upload,
  Link,
  Loader2,
  Clock,
  Download,
  Trash2,
  Copy,
  CheckCircle2,
  AlertCircle,
  Settings2,
  Cpu,
  Cloud,
  HardDrive,
  Zap,
  Star,
} from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { toast } from "sonner";
import type { TranscriptItem, RawTranscriptItem } from "@/lib/tauri";

interface TranscriptProgressPayload {
  id: string;
  progress?: number;
  status?: TranscriptItem["status"];
  text?: string;
  language?: string;
  error?: string;
}

function normalizeTranscriptStatus(status?: string): TranscriptItem["status"] {
  if (
    status === "pending" ||
    status === "processing" ||
    status === "completed" ||
    status === "error"
  ) {
    return status;
  }
  return "pending";
}

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // clipboard might be unavailable
    }
  };
  return (
    <button
      onClick={handleCopy}
      title="Copy to clipboard"
      className="flex-shrink-0 p-1.5 rounded hover:bg-muted transition-colors text-muted-foreground hover:text-foreground"
    >
      {copied ? (
        <CheckCircle2 className="w-3.5 h-3.5 text-green-500" />
      ) : (
        <Copy className="w-3.5 h-3.5" />
      )}
    </button>
  );
}

export function TranscribePage() {
  const { t } = useTranslation();
  const [url, setUrl] = useState("");
  const [items, setItems] = useState<TranscriptItem[]>([]);
  const [activeTab, setActiveTab] = useState("new");
  const [provider, setProvider] = useState<"api" | "local">("api");
  const [apiKey, setApiKey] = useState("");
  const [apiModel, setApiModel] = useState("whisper-1");
  const [selectedLocalModel, setSelectedLocalModel] = useState("whisper-base");
  const [showSetupDialog, setShowSetupDialog] = useState(false);
  const [isConfigured, setIsConfigured] = useState(false);
  const [setupSaving, setSetupSaving] = useState(false);
  const [setupError, setSetupError] = useState("");
  const [apiChecking, setApiChecking] = useState(false);
  const [apiCheckSuccess, setApiCheckSuccess] = useState(false);

  const mapTranscript = useCallback(
    (item: RawTranscriptItem): TranscriptItem => {
      const sourceValue = item.source || "";
      const sourceType = sourceValue.startsWith("http") ? "url" : "file";
      return {
        id: item.id || crypto.randomUUID(),
        title: item.title || sourceValue,
        source: sourceType,
        status: normalizeTranscriptStatus(item.status),
        progress: item.progress ?? 0,
        text: item.text || "",
        language: item.language || "",
        duration: item.durationSecs ? String(item.durationSecs) : "",
        createdAt: item.createdAt || "",
        error: item.error || "",
      };
    },
    [],
  );

  const refreshTranscripts = useCallback(async () => {
    const data = await commands.getTranscripts();
    setItems(data.map(mapTranscript));
  }, [mapTranscript]);

  useEffect(() => {
    refreshTranscripts().catch((err) =>
      console.error("Failed to load transcripts:", err),
    );
    commands
      .getSettings()
      .then((settings) => {
        const storedProvider =
          (settings.transcribe_provider as "api" | "local") || "api";
        setProvider(storedProvider);
        setApiKey(settings.openai_api_key || "");
        setApiModel(settings.openai_model || "whisper-1");
        const storedModel = settings.local_model_id || "whisper-base";
        const resolvedModel = LOCAL_MODELS.some((m) => m.id === storedModel)
          ? storedModel
          : "whisper-base";
        setSelectedLocalModel(resolvedModel);
        const configured = settings.transcription_configured === "true";
        setIsConfigured(configured);
        if (!configured) {
          setShowSetupDialog(true);
        }
      })
      .catch((err) =>
        console.error("Failed to load transcription settings:", err),
      );
  }, [refreshTranscripts]);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let cancelled = false;
    listen<TranscriptProgressPayload>("transcription-progress", (event) => {
      const payload = event.payload;
      setItems((prev) =>
        prev.map((item) =>
          item.id === payload.id
            ? {
                ...item,
                progress: payload.progress ?? item.progress,
                status: payload.status || item.status,
                text: payload.text ?? item.text,
                language: payload.language ?? item.language,
                error: payload.error ?? item.error,
              }
            : item,
        ),
      );
    }).then((unsub) => {
      if (cancelled) {
        unsub();
      } else {
        unlisten = unsub;
      }
    });
    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, []);

  const saveSetting = useCallback(async (key: string, value: string) => {
    await commands.saveSetting(key, value);
  }, []);

  const handleCheckApi = useCallback(async () => {
    setApiChecking(true);
    setSetupError("");
    setApiCheckSuccess(false);
    try {
      await commands.checkOpenaiTranscriptionApi(apiKey, apiModel);
      setApiCheckSuccess(true);
    } catch (err) {
      setSetupError(String(err));
    } finally {
      setApiChecking(false);
    }
  }, [apiKey, apiModel]);

  const handleSaveSetup = useCallback(async () => {
    setSetupSaving(true);
    setSetupError("");
    try {
      if (provider === "api") {
        await commands.checkOpenaiTranscriptionApi(apiKey, apiModel);
        setApiCheckSuccess(true);
      } else {
        await commands.installLocalTranscription(selectedLocalModel);
      }

      await saveSetting("transcription_configured", "true");
      setIsConfigured(true);
      setShowSetupDialog(false);
    } catch (err) {
      setSetupError(String(err));
    } finally {
      setSetupSaving(false);
    }
  }, [provider, apiKey, apiModel, selectedLocalModel, saveSetting]);

  const handleTranscribeUrl = useCallback(async () => {
    if (!url.trim()) return;
    if (!isConfigured) {
      setShowSetupDialog(true);
      return;
    }
    try {
      const source = url.trim();
      const id = await commands.startTranscription(
        source,
        provider === "api" ? apiModel : selectedLocalModel,
      );
      const item: TranscriptItem = {
        id,
        title: source,
        source: "url",
        status: "processing",
        progress: 0,
        createdAt: new Date().toISOString(),
      };
      setItems((prev) => [item, ...prev]);
      setUrl("");
      setActiveTab("history");
    } catch (err) {
      toast.error(t("transcribe.startFailed", { error: String(err) }));
    }
  }, [url, provider, apiModel, selectedLocalModel, isConfigured, t]);

  const handleFileUpload = useCallback(async () => {
    if (!isConfigured) {
      setShowSetupDialog(true);
      return;
    }
    const selected = await open({
      multiple: true,
      directory: false,
      filters: [
        {
          name: "Media",
          extensions: ["mp3", "wav", "m4a", "mp4", "mkv", "webm"],
        },
      ],
    });
    if (!selected) return;
    const files = Array.isArray(selected) ? selected : [selected];
    for (const filePath of files) {
      try {
        const id = await commands.startTranscription(
          filePath,
          provider === "api" ? apiModel : selectedLocalModel,
        );
        const fileName = filePath.split(/[/\\]/).pop() || filePath;
        setItems((prev) => [
          {
            id,
            title: fileName,
            source: "file",
            status: "processing",
            progress: 0,
            createdAt: new Date().toISOString(),
          },
          ...prev,
        ]);
      } catch (err) {
        toast.error(t("transcribe.fileStartFailed", { error: String(err) }));
      }
    }
    setActiveTab("history");
  }, [provider, apiModel, selectedLocalModel, isConfigured, t]);

  const handleDeleteTranscript = useCallback(
    async (id: string) => {
      try {
        await commands.deleteTranscript(id);
        setItems((prev) => prev.filter((item) => item.id !== id));
      } catch (err) {
        console.error("Failed to delete transcript:", err);
        toast.error(t("transcribe.startFailed", { error: String(err) }));
      }
    },
    [t],
  );

  const handleCopyText = useCallback(
    async (text?: string) => {
      if (!text) return;
      try {
        await navigator.clipboard.writeText(text);
        toast.success(t("common.done"));
      } catch (err) {
        console.error("Failed to copy text:", err);
      }
    },
    [t],
  );

  const handleDownloadText = useCallback((item: TranscriptItem) => {
    if (!item.text) return;
    const blob = new Blob([item.text], { type: "text/plain;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `${item.title || "transcript"}.txt`;
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
  }, []);

  return (
    <div className="flex flex-col h-full bg-background">
      {/* Header */}
      <div className="px-4 sm:px-6 pt-6 pb-2 sm:pb-4">
        <h1 className="text-2xl sm:text-3xl font-bold tracking-tight">
          {t("transcribe.title")}
        </h1>
        <p className="text-sm text-muted-foreground font-medium mt-1">
          {t("transcribe.subtitle")}
        </p>
      </div>

      <div className="px-4 sm:px-6 mb-5">
        <div className="relative overflow-hidden rounded-[24px] bg-card/60 backdrop-blur-md border border-border/50 dark:border-white/10 shadow-sm p-4 sm:p-5">
          <div className="absolute inset-0 bg-gradient-to-br from-primary/5 to-transparent pointer-events-none" />
          <div className="relative z-10 flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4">
            <div className="flex flex-wrap items-center gap-2 sm:gap-3">
              {provider === "api" ? (
                <div className="flex items-center gap-2 bg-blue-500/10 text-blue-500 px-3 py-1.5 rounded-xl border border-blue-500/20">
                  <Cloud className="w-4 h-4" />
                  <span className="text-xs font-semibold">
                    {t("transcribe.apiEngine")}
                  </span>
                  <span className="text-[10px] bg-background/50 px-1.5 py-0.5 rounded font-medium">
                    {apiModel}
                  </span>
                </div>
              ) : (
                <div className="flex items-center gap-2 bg-green-500/10 text-green-600 dark:text-green-400 px-3 py-1.5 rounded-xl border border-green-500/20">
                  <HardDrive className="w-4 h-4" />
                  <span className="text-xs font-semibold">
                    {t("transcribe.localEngine")}
                  </span>
                  <span className="text-[10px] bg-background/50 px-1.5 py-0.5 rounded font-medium">
                    {LOCAL_MODELS.find((m) => m.id === selectedLocalModel)
                      ?.name || selectedLocalModel}
                  </span>
                </div>
              )}
              {!isConfigured && (
                <span className="text-[10px] font-medium text-destructive bg-destructive/10 px-2 py-1 rounded-lg border border-destructive/20">
                  {t("transcribe.notConfigured")}
                </span>
              )}
            </div>
            <Button
              variant="outline"
              size="sm"
              onClick={() => setShowSetupDialog(true)}
              className="w-full sm:w-auto rounded-full h-10 shadow-sm border-border/50 bg-background/50 backdrop-blur-sm hover:bg-muted"
            >
              <Settings2 className="w-4 h-4 mr-2 text-muted-foreground" />
              {t("settings.title")}
            </Button>
          </div>
        </div>
      </div>

      {/* Transcription Setup Dialog */}
      <TranscriptionSetupDialog
        open={showSetupDialog}
        onOpenChange={setShowSetupDialog}
        provider={provider}
        onProviderChange={(v) => {
          setProvider(v);
          setSetupError("");
          saveSetting("transcribe_provider", v);
        }}
        apiKey={apiKey}
        onApiKeyChange={(v) => {
          setApiKey(v);
          setApiCheckSuccess(false);
          setSetupError("");
          saveSetting("openai_api_key", v);
        }}
        apiModel={apiModel}
        onApiModelChange={(v) => {
          setApiModel(v);
          setApiCheckSuccess(false);
          setSetupError("");
          saveSetting("openai_model", v);
        }}
        selectedLocalModel={selectedLocalModel}
        onLocalModelChange={(v) => {
          setSelectedLocalModel(v);
          setSetupError("");
          saveSetting("local_model_id", v);
        }}
        onCheckApi={handleCheckApi}
        apiChecking={apiChecking}
        apiCheckSuccess={apiCheckSuccess}
        setupError={setupError}
        saving={setupSaving}
        onSave={handleSaveSetup}
      />

      <div className="px-4 sm:px-6 flex-1 flex flex-col min-h-0 pb-6">
        <Tabs
          value={activeTab}
          onValueChange={setActiveTab}
          className="flex-1 flex flex-col min-h-0"
        >
          <TabsList className="grid w-full grid-cols-2 gap-2 bg-muted/50 p-1 rounded-full mb-5">
            <TabsTrigger
              value="new"
              className="w-full rounded-full data-[state=active]:bg-background data-[state=active]:shadow-sm"
            >
              {t("transcribe.new")}
            </TabsTrigger>
            <TabsTrigger
              value="history"
              className="w-full rounded-full data-[state=active]:bg-background data-[state=active]:shadow-sm"
            >
              {t("transcribe.history")}
              {items.length > 0 && (
                <span className="ml-1.5 text-[10px] font-bold bg-muted-foreground/20 text-muted-foreground px-1.5 py-0.5 rounded-full">
                  {items.length}
                </span>
              )}
            </TabsTrigger>
          </TabsList>

          {/* New Transcription */}
          <TabsContent
            value="new"
            className="flex-1 mt-0 data-[state=active]:animate-in data-[state=active]:fade-in-50 data-[state=active]:slide-in-from-bottom-2"
          >
            <div className="max-w-5xl mx-auto space-y-6 py-4 sm:py-8 px-4">
              <div className="grid grid-cols-1 lg:grid-cols-2 gap-5 items-stretch">
                {/* URL Input */}
                <div className="relative overflow-hidden rounded-[20px] bg-card/60 backdrop-blur-md border border-border/50 dark:border-white/10 shadow-sm p-4 sm:p-5 hover:border-white/20 transition-colors">
                  <div className="flex flex-col gap-5">
                    <div className="flex items-center gap-2 mb-1">
                      <div className="w-8 h-8 rounded-full bg-primary/10 flex items-center justify-center text-primary">
                        <Link className="w-5 h-5" />
                      </div>
                      <p className="text-sm font-semibold">
                        {t("transcribe.fromUrl")}
                      </p>
                    </div>
                    <div className="flex flex-col gap-3">
                      <Input
                        value={url}
                        onChange={(e) => setUrl(e.target.value)}
                        placeholder={t("transcribe.urlPlaceholder")}
                        onKeyDown={(e) =>
                          e.key === "Enter" && handleTranscribeUrl()
                        }
                        className="flex-1 h-12 rounded-full bg-background/50 border-border/50 text-base focus-visible:ring-primary/50"
                      />
                      <Button
                        onClick={handleTranscribeUrl}
                        disabled={!url.trim()}
                        className="h-12 rounded-full px-6 font-semibold shadow-sm w-full sm:w-auto"
                      >
                        <Mic className="w-4 h-4 mr-2" />
                        {t("transcribe.start")}
                      </Button>
                    </div>
                  </div>
                </div>

                {/* File Upload */}
                <div className="relative overflow-hidden rounded-[20px] bg-card/60 backdrop-blur-md border border-border/50 dark:border-white/10 shadow-sm p-4 sm:p-5 hover:border-white/20 transition-colors">
                  <div className="flex flex-col gap-4">
                    <div className="flex items-center gap-2">
                      <div className="w-8 h-8 rounded-full bg-primary/10 flex items-center justify-center text-primary">
                        <Upload className="w-4 h-4" />
                      </div>
                      <p className="text-sm font-semibold">
                        {t("transcribe.fromFile")}
                      </p>
                    </div>

                    <div
                      className="border-2 border-dashed border-border rounded-xl p-8 flex flex-col items-center justify-center text-center cursor-pointer hover:bg-muted/30 transition-colors"
                      onClick={handleFileUpload}
                    >
                      <div className="w-12 h-12 rounded-full bg-muted flex items-center justify-center mb-3">
                        <Upload className="w-5 h-5 text-muted-foreground" />
                      </div>
                      <p className="font-semibold text-sm mb-1">
                        {t("transcribe.selectFiles")}
                      </p>
                      <p className="text-xs text-muted-foreground/70 font-medium max-w-[200px]">
                        {t("transcribe.supportedFormats")}
                      </p>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </TabsContent>

          {/* History */}
          <TabsContent value="history" className="flex-1 mt-4 min-h-0">
            {items.length === 0 ? (
              <div className="flex flex-col items-center justify-center h-64 text-muted-foreground">
                <Clock className="w-12 h-12 mb-3 opacity-30" />
                <p className="text-sm">{t("transcribe.noHistory")}</p>
              </div>
            ) : (
              <ScrollArea className="h-full">
                <div className="space-y-3 pr-3">
                  {items.map((item) => (
                    <TranscriptCard
                      key={item.id}
                      item={item}
                      onCopy={handleCopyText}
                      onDownload={handleDownloadText}
                      onDelete={handleDeleteTranscript}
                    />
                  ))}
                </div>
              </ScrollArea>
            )}
          </TabsContent>
        </Tabs>
      </div>
    </div>
  );
}

function TranscriptCard({
  item,
  onCopy,
  onDownload,
  onDelete,
}: {
  item: TranscriptItem;
  onCopy: (text?: string) => void;
  onDownload: (item: TranscriptItem) => void;
  onDelete: (id: string) => void;
}) {
  const { t } = useTranslation();

  return (
    <div className="relative overflow-hidden rounded-[20px] bg-card/60 backdrop-blur-md border border-border/50 dark:border-white/10 shadow-sm p-4 hover:bg-card/80 transition-colors">
      <div className="flex items-start justify-between gap-3">
        <div className="flex-1 min-w-0">
          <div className="flex flex-wrap items-center gap-2 mb-2">
            <Badge
              variant={item.source === "url" ? "default" : "secondary"}
              className="rounded-md px-1.5 py-0"
            >
              {item.source === "url" ? "URL" : t("transcribe.file")}
            </Badge>
            {item.status === "completed" && (
              <span className="flex items-center text-[10px] font-medium text-green-500 bg-green-500/10 px-1.5 py-0.5 rounded-md border border-green-500/20">
                <CheckCircle2 className="w-3 h-3 mr-1" />
                {t("common.done")}
              </span>
            )}
            {item.status === "error" && (
              <span className="flex items-center text-[10px] font-medium text-destructive bg-destructive/10 px-1.5 py-0.5 rounded-md border border-destructive/20">
                <AlertCircle className="w-3 h-3 mr-1" />
                {t("common.error")}
              </span>
            )}
            {(item.status === "pending" || item.status === "processing") && (
              <span className="flex items-center text-[10px] font-medium text-yellow-500 bg-yellow-500/10 px-1.5 py-0.5 rounded-md border border-yellow-500/20">
                <Loader2 className="w-3 h-3 mr-1 animate-spin" />
                {t("transcribe.processing")}
              </span>
            )}
          </div>
          <p className="text-sm font-semibold truncate text-foreground mb-1">
            {item.title}
          </p>
          {item.language && (
            <p className="text-xs font-medium text-muted-foreground">
              {t("transcribe.detectedLang")}:{" "}
              <span className="text-foreground/80">{item.language}</span>
            </p>
          )}
          {(item.status === "pending" || item.status === "processing") && (
            <div className="mt-3">
              <div className="flex justify-between text-[10px] text-muted-foreground mb-1 font-medium">
                <span>{Math.round(item.progress)}%</span>
              </div>
              <Progress value={item.progress} className="h-1.5 bg-muted/50" />
            </div>
          )}
          {item.status === "error" && item.error && (
            <div className="mt-3 p-2 rounded-lg bg-destructive/10 border border-destructive/20">
              <p className="text-xs font-medium text-destructive line-clamp-3">
                {item.error}
              </p>
            </div>
          )}
          {item.text && (
            <div className="mt-3">
              <p className="text-xs text-muted-foreground line-clamp-3 leading-relaxed">
                {item.text}
              </p>
            </div>
          )}
        </div>

        <div className="flex flex-col gap-1 items-end shrink-0">
          {item.status === "completed" && (
            <div className="flex gap-1">
              <Button
                variant="ghost"
                size="icon"
                className="h-8 w-8 rounded-lg hover:bg-muted"
                onClick={() => onCopy(item.text)}
              >
                <Copy className="w-4 h-4 text-muted-foreground" />
              </Button>
              <Button
                variant="ghost"
                size="icon"
                className="h-8 w-8 rounded-lg hover:bg-muted"
                onClick={() => onDownload(item)}
              >
                <Download className="w-4 h-4 text-muted-foreground" />
              </Button>
            </div>
          )}
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8 rounded-lg hover:bg-destructive/10 hover:text-destructive text-muted-foreground"
            onClick={() => onDelete(item.id)}
          >
            <Trash2 className="w-4 h-4" />
          </Button>
        </div>
      </div>
    </div>
  );
}

/* ─── Local Model Definitions ─────────────────────────── */

interface LocalModel {
  id: string;
  name: string;
  engine: string;
  size: string;
  speed: number; // 1-5
  quality: number; // 1-5
  languages: string;
  description: string;
}

const LOCAL_MODELS: LocalModel[] = [
  {
    id: "whisper-tiny",
    name: "Whisper Tiny",
    engine: "whisper.cpp",
    size: "~75 MB",
    speed: 5,
    quality: 1,
    languages: "99 languages",
    description: "Fastest model, good for quick drafts and testing",
  },
  {
    id: "whisper-base",
    name: "Whisper Base",
    engine: "whisper.cpp",
    size: "~142 MB",
    speed: 4,
    quality: 2,
    languages: "99 languages",
    description: "Good balance for clear speech in quiet environments",
  },
  {
    id: "whisper-small",
    name: "Whisper Small",
    engine: "whisper.cpp",
    size: "~466 MB",
    speed: 3,
    quality: 3,
    languages: "99 languages",
    description: "Balanced speed and quality for most use cases",
  },
  {
    id: "whisper-medium",
    name: "Whisper Medium",
    engine: "whisper.cpp",
    size: "~1.5 GB",
    speed: 2,
    quality: 4,
    languages: "99 languages",
    description: "High accuracy, good for noisy audio and accents",
  },
  {
    id: "whisper-large-v3",
    name: "Whisper Large v3",
    engine: "whisper.cpp",
    size: "~3 GB",
    speed: 1,
    quality: 5,
    languages: "99 languages",
    description: "Best accuracy, ideal for professional transcription",
  },
  {
    id: "distil-whisper-large-v3",
    name: "Distil Whisper Large v3",
    engine: "whisper.cpp",
    size: "~756 MB",
    speed: 4,
    quality: 4,
    languages: "English-focused",
    description: "Distilled model — 6x faster, nearly same accuracy as Large",
  },
];

/* ─── Speed/Quality Rating Component ─────────────────── */

function RatingDots({
  value,
  max = 5,
  color,
}: {
  value: number;
  max?: number;
  color: string;
}) {
  return (
    <div className="flex gap-0.5">
      {Array.from({ length: max }, (_, i) => (
        <div
          key={i}
          className={`w-1.5 h-1.5 rounded-full ${
            i < value ? color : "bg-muted-foreground/20"
          }`}
        />
      ))}
    </div>
  );
}

/* ─── Transcription Setup Dialog ─────────────────────── */

function TranscriptionSetupDialog({
  open,
  onOpenChange,
  provider,
  onProviderChange,
  apiKey,
  onApiKeyChange,
  apiModel,
  onApiModelChange,
  selectedLocalModel,
  onLocalModelChange,
  onCheckApi,
  apiChecking,
  apiCheckSuccess,
  setupError,
  saving,
  onSave,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  provider: "api" | "local";
  onProviderChange: (v: "api" | "local") => void;
  apiKey: string;
  onApiKeyChange: (v: string) => void;
  apiModel: string;
  onApiModelChange: (v: string) => void;
  selectedLocalModel: string;
  onLocalModelChange: (v: string) => void;
  onCheckApi: () => Promise<void>;
  apiChecking: boolean;
  apiCheckSuccess: boolean;
  setupError: string;
  saving: boolean;
  onSave: () => Promise<void>;
}) {
  const { t } = useTranslation();
  const manualInstallCommandMatch = setupError.match(
    /Please install it manually(?: in Termux)?:\s*([^\n]+)/i,
  );
  const manualInstallCommand = manualInstallCommandMatch?.[1]?.trim() ?? "";
  const setupErrorText = manualInstallCommand
    ? setupError
        .replace(/Please install it manually(?: in Termux)?:\s*([^\n]+)/i, "")
        .trim()
    : setupError;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[600px] max-h-[85vh] flex flex-col">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Settings2 className="w-5 h-5" />
            {t("transcribe.setupTitle")}
          </DialogTitle>
          <DialogDescription>{t("transcribe.setupDesc")}</DialogDescription>
        </DialogHeader>

        <div className="flex-1 overflow-auto space-y-4 py-2">
          {/* Engine Selection */}
          <div>
            <Label className="text-sm font-medium mb-2 block">
              {t("transcribe.engine")}
            </Label>
            <Tabs
              value={provider}
              onValueChange={(v) => {
                onProviderChange(v as "api" | "local");
              }}
            >
              <TabsList className="grid grid-cols-2 w-full">
                <TabsTrigger value="api" className="gap-2">
                  <Cloud className="w-4 h-4" />
                  {t("transcribe.apiCloud")}
                </TabsTrigger>
                <TabsTrigger value="local" className="gap-2">
                  <Cpu className="w-4 h-4" />
                  {t("transcribe.localOffline")}
                </TabsTrigger>
              </TabsList>
            </Tabs>
          </div>

          {provider === "api" ? (
            <div className="space-y-4">
              <div className="rounded-lg border p-3 bg-blue-500/5">
                <p className="text-xs text-muted-foreground">
                  {t("transcribe.apiDesc")}
                </p>
              </div>
              <div className="space-y-1.5">
                <Label>{t("transcribe.apiKeyLabel")}</Label>
                <Input
                  type="password"
                  value={apiKey}
                  onChange={(e) => onApiKeyChange(e.target.value)}
                  placeholder="sk-..."
                />
              </div>
              <div className="space-y-1.5">
                <Label>{t("transcribe.model")}</Label>
                <Input
                  value={apiModel}
                  onChange={(e) => onApiModelChange(e.target.value)}
                  placeholder="whisper-1"
                />
              </div>
              <div className="flex items-center gap-2">
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => void onCheckApi()}
                  disabled={apiChecking || !apiKey.trim()}
                >
                  {apiChecking ? (
                    <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" />
                  ) : (
                    <Cloud className="w-3.5 h-3.5 mr-1.5" />
                  )}
                  {t("transcribe.testApi")}
                </Button>
                {apiCheckSuccess && (
                  <Badge variant="success">
                    <CheckCircle2 className="w-3 h-3 mr-1" />
                    {t("transcribe.apiOk")}
                  </Badge>
                )}
              </div>
            </div>
          ) : (
            <div className="space-y-3">
              <div className="rounded-lg border p-3 bg-green-500/5">
                <p className="text-xs text-muted-foreground">
                  {t("transcribe.localDesc")}
                </p>
              </div>

              {/* Model legend */}
              <div className="flex items-center gap-4 text-[10px] text-muted-foreground px-1">
                <div className="flex items-center gap-1">
                  <Zap className="w-3 h-3" /> Speed
                </div>
                <div className="flex items-center gap-1">
                  <Star className="w-3 h-3" /> Quality
                </div>
              </div>

              {/* Model list */}
              <ScrollArea className="max-h-auto">
                <div className="space-y-2 pr-3">
                  {LOCAL_MODELS.map((model) => (
                    <button
                      key={model.id}
                      onClick={() => onLocalModelChange(model.id)}
                      className={`w-full text-left p-3 rounded-lg border transition-colors ${
                        selectedLocalModel === model.id
                          ? "border-primary bg-primary/5 ring-1 ring-primary/30"
                          : "border-border hover:bg-accent/50"
                      }`}
                    >
                      <div className="flex items-start justify-between gap-2">
                        <div className="flex-1 min-w-0">
                          <div className="flex items-center gap-2">
                            <span className="text-sm font-medium">
                              {model.name}
                            </span>
                            <Badge
                              variant="outline"
                              className="text-[9px] px-1.5 py-0"
                            >
                              {model.size}
                            </Badge>
                          </div>
                          <p className="text-[11px] text-muted-foreground mt-0.5">
                            {model.description}
                          </p>
                          <div className="flex items-center gap-1 mt-1">
                            <Badge
                              variant="secondary"
                              className="text-[9px] px-1 py-0"
                            >
                              {model.engine}
                            </Badge>
                            <Badge
                              variant="secondary"
                              className="text-[9px] px-1 py-0"
                            >
                              {model.languages}
                            </Badge>
                          </div>
                        </div>
                        <div className="flex flex-col gap-1.5 items-end flex-shrink-0">
                          <div className="flex items-center gap-1.5">
                            <Zap className="w-3 h-3 text-yellow-500" />
                            <RatingDots
                              value={model.speed}
                              color="bg-yellow-500"
                            />
                          </div>
                          <div className="flex items-center gap-1.5">
                            <Star className="w-3 h-3 text-blue-500" />
                            <RatingDots
                              value={model.quality}
                              color="bg-blue-500"
                            />
                          </div>
                        </div>
                      </div>
                    </button>
                  ))}
                </div>
              </ScrollArea>
              <p className="text-xs text-muted-foreground">
                {t("transcribe.localSetupNote")}
              </p>
            </div>
          )}

          {setupError && (
            <div className="rounded-lg border border-destructive/30 bg-destructive/10 p-3 space-y-2">
              <p className="text-xs text-destructive flex-1 break-words">
                {setupErrorText}
              </p>
              {manualInstallCommand && (
                <div className="space-y-1">
                  <p className="text-[11px] text-muted-foreground">
                    Manual command
                  </p>
                  <div className="flex items-start gap-1 rounded-md bg-muted/70 px-3 py-2">
                    <pre className="flex-1 text-xs whitespace-pre-wrap break-all leading-relaxed text-foreground">
                      {manualInstallCommand}
                    </pre>
                    <CopyButton text={manualInstallCommand} />
                  </div>
                </div>
              )}
            </div>
          )}
        </div>

        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={saving}
          >
            {t("common.cancel")}
          </Button>
          <Button
            onClick={() => void onSave()}
            disabled={saving || apiChecking}
          >
            {saving && <Loader2 className="w-4 h-4 mr-1.5 animate-spin" />}
            {t("transcribe.saveAndContinue")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
