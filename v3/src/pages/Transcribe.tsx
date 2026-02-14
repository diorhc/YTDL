import { useTranslation } from "react-i18next";
import { useState, useCallback, useEffect } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent } from "@/components/ui/card";
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

interface TranscriptItem {
  id: string;
  title: string;
  source: "url" | "file";
  status: "pending" | "processing" | "completed" | "error";
  progress: number;
  text?: string;
  language?: string;
  duration?: string;
  createdAt: string;
  error?: string;
}

interface RawTranscriptItem {
  id?: string;
  title?: string;
  source?: string;
  status?: string;
  progress?: number;
  text?: string;
  language?: string;
  durationSecs?: number;
  createdAt?: string;
  error?: string;
}

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
    refreshTranscripts();
    commands.getSettings().then((settings) => {
      setProvider((settings.transcribe_provider as "api" | "local") || "api");
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
    });
  }, [refreshTranscripts]);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
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
      unlisten = unsub;
    });
    return () => {
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
      toast.error(`Transcription failed to start: ${String(err)}`);
    }
  }, [url, provider, apiModel, selectedLocalModel, isConfigured]);

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
        toast.error(`Failed to start transcription for file: ${String(err)}`);
      }
    }
    setActiveTab("history");
  }, [provider, apiModel, selectedLocalModel, isConfigured]);

  const handleDeleteTranscript = useCallback(async (id: string) => {
    await commands.deleteTranscript(id);
    setItems((prev) => prev.filter((item) => item.id !== id));
  }, []);

  const handleCopyText = useCallback(async (text?: string) => {
    if (!text) return;
    await navigator.clipboard.writeText(text);
  }, []);

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
    <div className="flex flex-col h-full p-6">
      <div className="mb-6">
        <h1 className="text-2xl font-bold">{t("transcribe.title")}</h1>
        <p className="text-sm text-muted-foreground mt-1">
          {t("transcribe.subtitle")}
        </p>
      </div>

      <Card className="mb-4">
        <CardContent className="p-3">
          <div className="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4">
            <div className="flex flex-wrap items-center gap-3">
              {provider === "api" ? (
                <div className="flex items-center gap-2">
                  <Cloud className="w-4 h-4 text-blue-500" />
                  <span className="text-sm font-medium">API Engine</span>
                  <Badge variant="secondary" className="text-[10px]">
                    {apiModel}
                  </Badge>
                </div>
              ) : (
                <div className="flex items-center gap-2">
                  <HardDrive className="w-4 h-4 text-green-500" />
                  <span className="text-sm font-medium">Local Engine</span>
                  <Badge variant="secondary" className="text-[10px]">
                    {LOCAL_MODELS.find((m) => m.id === selectedLocalModel)
                      ?.name || selectedLocalModel}
                  </Badge>
                </div>
              )}
              {!isConfigured && (
                <Badge variant="destructive" className="text-[10px]">
                  Not configured
                </Badge>
              )}
            </div>
            <Button
              variant="outline"
              size="sm"
              onClick={() => setShowSetupDialog(true)}
              className="w-full sm:w-auto"
            >
              <Settings2 className="w-3.5 h-3.5 mr-1.5" />
              Settings
            </Button>
          </div>
        </CardContent>
      </Card>

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

      <Tabs
        value={activeTab}
        onValueChange={setActiveTab}
        className="flex-1 flex flex-col min-h-0"
      >
        <TabsList className="grid w-full grid-cols-2 gap-2">
          <TabsTrigger value="new" className="w-full">
            {t("transcribe.new")}
          </TabsTrigger>
          <TabsTrigger value="history" className="w-full">
            {t("transcribe.history")}
            {items.length > 0 && (
              <Badge variant="secondary" className="ml-1.5 text-xs px-1.5 py-0">
                {items.length}
              </Badge>
            )}
          </TabsTrigger>
        </TabsList>

        {/* New Transcription */}
        <TabsContent value="new" className="flex-1 mt-4">
          <div className="max-w-xl mx-auto space-y-6 py-8">
            <p className="text-center text-muted-foreground text-sm">
              {t("transcribe.description")}
            </p>

            {/* URL Input */}
            <Card>
              <CardContent className="p-4 space-y-3">
                <div className="flex items-center gap-2">
                  <Link className="w-4 h-4 text-muted-foreground flex-shrink-0" />
                  <p className="text-sm font-medium">
                    {t("transcribe.fromUrl")}
                  </p>
                </div>
                <div className="flex gap-2">
                  <Input
                    value={url}
                    onChange={(e) => setUrl(e.target.value)}
                    placeholder={t("transcribe.urlPlaceholder")}
                    onKeyDown={(e) =>
                      e.key === "Enter" && handleTranscribeUrl()
                    }
                  />
                  <Button onClick={handleTranscribeUrl} disabled={!url.trim()}>
                    <Mic className="w-4 h-4 mr-1.5" />
                    {t("transcribe.start")}
                  </Button>
                </div>
              </CardContent>
            </Card>

            {/* File Upload */}
            <Card>
              <CardContent className="p-4 space-y-3">
                <div className="flex items-center gap-2">
                  <Upload className="w-4 h-4 text-muted-foreground flex-shrink-0" />
                  <p className="text-sm font-medium">
                    {t("transcribe.fromFile")}
                  </p>
                </div>
                <Button
                  variant="outline"
                  className="w-full"
                  onClick={handleFileUpload}
                >
                  <Upload className="w-4 h-4 mr-1.5" />
                  {t("transcribe.selectFiles")}
                </Button>
                <p className="text-xs text-muted-foreground text-center">
                  {t("transcribe.supportedFormats")}
                </p>
              </CardContent>
            </Card>
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
    <Card>
      <CardContent className="p-4">
        <div className="flex items-start justify-between gap-3">
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2 mb-1">
              <Badge variant={item.source === "url" ? "default" : "secondary"}>
                {item.source === "url" ? "URL" : t("transcribe.file")}
              </Badge>
              {item.status === "completed" && (
                <Badge variant="success">
                  <CheckCircle2 className="w-3 h-3 mr-1" />
                  {t("common.done")}
                </Badge>
              )}
              {item.status === "error" && (
                <Badge variant="destructive">
                  <AlertCircle className="w-3 h-3 mr-1" />
                  {t("common.error")}
                </Badge>
              )}
              {(item.status === "pending" || item.status === "processing") && (
                <Badge variant="warning">
                  <Loader2 className="w-3 h-3 mr-1 animate-spin" />
                  {t("transcribe.processing")}
                </Badge>
              )}
            </div>
            <p className="text-sm font-medium truncate">{item.title}</p>
            {item.language && (
              <p className="text-xs text-muted-foreground mt-0.5">
                {t("transcribe.detectedLang")}: {item.language}
              </p>
            )}
            {(item.status === "pending" || item.status === "processing") && (
              <Progress value={item.progress} className="mt-2 h-1.5" />
            )}
            {item.status === "error" && item.error && (
              <p className="text-xs text-destructive mt-2 line-clamp-3">
                {item.error}
              </p>
            )}
            {item.text && (
              <p className="text-xs text-muted-foreground mt-2 line-clamp-3">
                {item.text}
              </p>
            )}
          </div>

          <div className="flex gap-1">
            {item.status === "completed" && (
              <>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => onCopy(item.text)}
                >
                  <Copy className="w-4 h-4" />
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => onDownload(item)}
                >
                  <Download className="w-4 h-4" />
                </Button>
              </>
            )}
            <Button variant="ghost" size="sm" onClick={() => onDelete(item.id)}>
              <Trash2 className="w-4 h-4" />
            </Button>
          </div>
        </div>
      </CardContent>
    </Card>
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
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[600px] max-h-[85vh] flex flex-col">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Settings2 className="w-5 h-5" />
            Transcription Setup
          </DialogTitle>
          <DialogDescription>
            Choose your transcription engine and configure it before getting
            started.
          </DialogDescription>
        </DialogHeader>

        <div className="flex-1 overflow-auto space-y-4 py-2">
          {/* Engine Selection */}
          <div>
            <Label className="text-sm font-medium mb-2 block">Engine</Label>
            <Tabs
              value={provider}
              onValueChange={(v) => onProviderChange(v as "api" | "local")}
            >
              <TabsList className="grid grid-cols-2 w-full">
                <TabsTrigger value="api" className="gap-2">
                  <Cloud className="w-4 h-4" />
                  API (Cloud)
                </TabsTrigger>
                <TabsTrigger value="local" className="gap-2">
                  <Cpu className="w-4 h-4" />
                  Local (Offline)
                </TabsTrigger>
              </TabsList>
            </Tabs>
          </div>

          {provider === "api" ? (
            <div className="space-y-4">
              <div className="rounded-lg border p-3 bg-blue-500/5">
                <p className="text-xs text-muted-foreground">
                  Uses cloud-based AI services for transcription. Requires an
                  API key and internet connection. Fast and accurate, with
                  per-usage billing.
                </p>
              </div>
              <div className="space-y-1.5">
                <Label>OpenAI API Key</Label>
                <Input
                  type="password"
                  value={apiKey}
                  onChange={(e) => onApiKeyChange(e.target.value)}
                  placeholder="sk-..."
                />
              </div>
              <div className="space-y-1.5">
                <Label>Model</Label>
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
                  Test API
                </Button>
                {apiCheckSuccess && (
                  <Badge variant="success">
                    <CheckCircle2 className="w-3 h-3 mr-1" />
                    API OK
                  </Badge>
                )}
              </div>
            </div>
          ) : (
            <div className="space-y-3">
              <div className="rounded-lg border p-3 bg-green-500/5">
                <p className="text-xs text-muted-foreground">
                  Runs entirely on your device — no internet needed, fully
                  private. Choose a model below based on your needs and hardware
                  capabilities.
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
                Local setup will download `whisper.cpp` runtime and selected
                model automatically.
              </p>
            </div>
          )}

          {setupError && (
            <div className="rounded-lg border border-destructive/30 bg-destructive/10 p-3">
              <p className="text-xs text-destructive">{setupError}</p>
            </div>
          )}
        </div>

        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={saving}
          >
            Cancel
          </Button>
          <Button
            onClick={() => void onSave()}
            disabled={saving || apiChecking}
          >
            {saving && <Loader2 className="w-4 h-4 mr-1.5 animate-spin" />}
            Save & Continue
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
