// Every display string lives here, pt-BR and English -- the same rule `crates/ui/src/i18n.rs`
// enforces for the egui shell. Wording is reused from that file where the concept matches so
// both shells read the same while they coexist.

export type Locale = "pt-BR" | "en";

const strings = {
  untitledProject: { "pt-BR": "Sem título", en: "Untitled" },
  defaultSequenceName: { "pt-BR": "Sequência 1", en: "Sequence 1" },
  homeTitle: { "pt-BR": "Projetos recentes", en: "Recent projects" },
  homeSubtitle: {
    "pt-BR": "Continue de onde parou ou comece um projeto novo.",
    en: "Continue where you left off or start a new project.",
  },
  newProject: { "pt-BR": "Novo projeto", en: "New project" },
  openProject: { "pt-BR": "Abrir projeto…", en: "Open project…" },
  backToEditor: { "pt-BR": "Voltar ao editor", en: "Back to editor" },
  home: { "pt-BR": "Início", en: "Home" },
  save: { "pt-BR": "Salvar", en: "Save" },
  saveAs: { "pt-BR": "Salvar como…", en: "Save as…" },
  undo: { "pt-BR": "Desfazer", en: "Undo" },
  redo: { "pt-BR": "Refazer", en: "Redo" },
  unsaved: { "pt-BR": "alterações não salvas", en: "unsaved changes" },

  mediaLibrary: { "pt-BR": "Biblioteca de mídia", en: "Media library" },
  importFiles: { "pt-BR": "Importar arquivos", en: "Import files" },
  libraryEmpty: {
    "pt-BR": "Nenhum arquivo importado neste projeto ainda.",
    en: "No files imported into this project yet.",
  },
  dropToImport: { "pt-BR": "Solte para importar", en: "Drop to import" },
  addToTimeline: { "pt-BR": "Adicionar à timeline", en: "Add to timeline" },
  favorite: { "pt-BR": "Favorito", en: "Favorite" },
  analyzing: { "pt-BR": "analisando…", en: "analyzing…" },

  previewUnavailable: { "pt-BR": "Pré-visualização indisponível", en: "Preview unavailable" },
  previewEmpty: {
    "pt-BR": "Nada na timeline nesta posição",
    en: "Nothing on the timeline at this position",
  },
  play: { "pt-BR": "Reproduzir", en: "Play" },
  pause: { "pt-BR": "Pausar", en: "Pause" },
  stepBack: { "pt-BR": "Quadro anterior", en: "Previous frame" },
  stepForward: { "pt-BR": "Próximo quadro", en: "Next frame" },

  timeline: { "pt-BR": "TIMELINE", en: "TIMELINE" },
  timelineEmpty: {
    "pt-BR": "Este projeto ainda não tem clipes na timeline.",
    en: "This project doesn't have any clips on the timeline yet.",
  },
  split: { "pt-BR": "Cortar no cursor", en: "Split at playhead" },
  delete: { "pt-BR": "Excluir", en: "Delete" },
  rippleDelete: { "pt-BR": "Excluir e fechar lacuna", en: "Ripple delete" },
  addVideoTrack: { "pt-BR": "Adicionar faixa de vídeo", en: "Add video track" },
  addTextClip: { "pt-BR": "Adicionar texto", en: "Add text" },
  addShapeClip: { "pt-BR": "Adicionar forma", en: "Add shape" },
  addAudioTrack: { "pt-BR": "Adicionar faixa de áudio", en: "Add audio track" },
  hideTrack: { "pt-BR": "Ocultar faixa", en: "Hide track" },
  showTrack: { "pt-BR": "Mostrar faixa", en: "Show track" },
  lockTrack: { "pt-BR": "Travar faixa", en: "Lock track" },
  unlockTrack: { "pt-BR": "Destravar faixa", en: "Unlock track" },
  zoomIn: { "pt-BR": "Aumentar zoom", en: "Zoom in" },
  zoomOut: { "pt-BR": "Diminuir zoom", en: "Zoom out" },
  sequences: { "pt-BR": "Sequências", en: "Sequences" },
  sequence: { "pt-BR": "Sequência", en: "Sequence" },
  copy: { "pt-BR": "cópia", en: "copy" },
  addSequence: { "pt-BR": "Adicionar sequência", en: "Add sequence" },
  duplicateSequence: { "pt-BR": "Duplicar sequência", en: "Duplicate sequence" },
  renameSequence: { "pt-BR": "Renomear sequência", en: "Rename sequence" },
  removeSequence: { "pt-BR": "Remover sequência", en: "Remove sequence" },
  moveSequenceLeft: { "pt-BR": "Mover sequência à esquerda", en: "Move sequence left" },
  moveSequenceRight: { "pt-BR": "Mover sequência à direita", en: "Move sequence right" },
  markers: { "pt-BR": "Marcadores", en: "Markers" },
  addMarker: { "pt-BR": "Adicionar marcador", en: "Add marker" },
  removeMarker: { "pt-BR": "Remover marcador", en: "Remove marker" },
  seekMarker: { "pt-BR": "Ir para marcador", en: "Seek marker" },
  markerLabel: { "pt-BR": "Rótulo do marcador", en: "Marker label" },
  markerKind: { "pt-BR": "Tipo de marcador", en: "Marker kind" },
  markerCompleted: { "pt-BR": "Marcador concluído", en: "Marker completed" },
  previousMarker: { "pt-BR": "Marcador anterior", en: "Previous marker" },
  nextMarker: { "pt-BR": "Próximo marcador", en: "Next marker" },

  inspector: { "pt-BR": "Inspetor", en: "Inspector" },
  noClipSelected: { "pt-BR": "Nenhum clipe selecionado", en: "No clip selected" },
  gain: { "pt-BR": "Ganho", en: "Gain" },
  speed: { "pt-BR": "Velocidade", en: "Speed" },
  brightness: { "pt-BR": "Brilho", en: "Brightness" },
  contrast: { "pt-BR": "Contraste", en: "Contrast" },
  saturation: { "pt-BR": "Saturação", en: "Saturation" },
  freeze: { "pt-BR": "Congelar quadro", en: "Freeze frame" },
  flip: { "pt-BR": "Espelhar horizontalmente", en: "Flip horizontally" },
  voiceCleanup: { "pt-BR": "Limpeza de voz", en: "Voice cleanup" },
  source: { "pt-BR": "Origem", en: "Source" },
  loudness: { "pt-BR": "Loudness", en: "Loudness" },
  text: { "pt-BR": "Texto", en: "Text" },
  defaultTextContent: { "pt-BR": "Texto", en: "Text" },
  textContentHint: { "pt-BR": "Digite seu texto", en: "Type your text" },
  font: { "pt-BR": "Fonte", en: "Font" },
  fontStyle: { "pt-BR": "Estilo", en: "Style" },
  fontRegular: { "pt-BR": "Regular", en: "Regular" },
  fontBold: { "pt-BR": "Negrito", en: "Bold" },
  fontSize: { "pt-BR": "Tamanho da fonte", en: "Font size" },
  unknownFont: { "pt-BR": "Fonte desconhecida", en: "Unknown font" },
  fontCategorySans: { "pt-BR": "Sem serifa", en: "Sans" },
  fontCategoryDisplay: { "pt-BR": "Destaque", en: "Display" },
  fontCategorySerif: { "pt-BR": "Serifada", en: "Serif" },
  fontCategoryHandwritten: { "pt-BR": "Manuscrita", en: "Handwritten" },
  fontCategoryMonospace: { "pt-BR": "Monoespaçada", en: "Monospace" },
  fontCategoryInternational: { "pt-BR": "Internacional", en: "International" },
  textColor: { "pt-BR": "Cor do texto", en: "Text color" },
  alignment: { "pt-BR": "Alinhamento", en: "Alignment" },
  alignAuto: { "pt-BR": "Automático", en: "Auto" },
  alignLeft: { "pt-BR": "Esquerda", en: "Left" },
  alignCenter: { "pt-BR": "Centro", en: "Center" },
  alignRight: { "pt-BR": "Direita", en: "Right" },
  alignStart: { "pt-BR": "Início", en: "Start" },
  alignEnd: { "pt-BR": "Fim", en: "End" },
  positionX: { "pt-BR": "Posição X", en: "Position X" },
  positionY: { "pt-BR": "Posição Y", en: "Position Y" },
  wordHighlight: { "pt-BR": "Destacar palavra falada", en: "Highlight spoken word" },
  highlightColor: { "pt-BR": "Cor do destaque", en: "Highlight color" },
  shape: { "pt-BR": "Forma", en: "Shape" },
  shapeKind: { "pt-BR": "Tipo", en: "Kind" },
  shapeColor: { "pt-BR": "Cor", en: "Color" },
  width: { "pt-BR": "Largura", en: "Width" },
  height: { "pt-BR": "Altura", en: "Height" },
  opacity: { "pt-BR": "Opacidade", en: "Opacity" },
  shapeEllipse: { "pt-BR": "Elipse/círculo", en: "Ellipse/circle" },
  shapeRectangle: { "pt-BR": "Retângulo/quadrado", en: "Rectangle/square" },
  shapeTriangle: { "pt-BR": "Triângulo", en: "Triangle" },
  shapeTrapezoid: { "pt-BR": "Trapézio", en: "Trapezoid" },
  shapeArrow: { "pt-BR": "Seta", en: "Arrow" },
  shapeCustom: { "pt-BR": "Personalizado", en: "Custom" },

  export: { "pt-BR": "Exportar", en: "Export" },
  exportQueue: { "pt-BR": "Fila de exportação", en: "Export queue" },
  exportTarget: { "pt-BR": "Normalização", en: "Normalization" },
  aspectRatio: { "pt-BR": "Proporção", en: "Aspect ratio" },
  aspectOriginal: { "pt-BR": "Original", en: "Original" },
  aspectLandscape: { "pt-BR": "16:9", en: "16:9" },
  aspectPortrait: { "pt-BR": "9:16", en: "9:16" },
  aspectSquare: { "pt-BR": "1:1", en: "1:1" },
  statusRendering: { "pt-BR": "Renderizando", en: "Rendering" },
  statusDone: { "pt-BR": "Concluído", en: "Done" },
  statusFailed: { "pt-BR": "Falhou", en: "Failed" },
  statusCancelled: { "pt-BR": "Cancelado", en: "Cancelled" },
  cancel: { "pt-BR": "Cancelar", en: "Cancel" },
  exportOverwriteConfirm: {
    "pt-BR": "O arquivo já existe. Substituir? (Cancelar cria uma cópia com novo nome.)",
    en: "The file already exists. Overwrite it? (Cancel creates a renamed copy.)",
  },
  noExports: { "pt-BR": "Nenhuma exportação nesta sessão.", en: "No exports this session." },
  errorPrefix: { "pt-BR": "Erro", en: "Error" },
  preferences: { "pt-BR": "Ajustes", en: "Preferences" },
  language: { "pt-BR": "Idioma", en: "Language" },
  defaultLufs: { "pt-BR": "Perfil LUFS padrão", en: "Default LUFS profile" },
  hardwareDecode: { "pt-BR": "Decodificação por hardware", en: "Hardware decoding" },
  outputFolder: { "pt-BR": "Pasta de saída", en: "Output folder" },
  autosaveInterval: { "pt-BR": "Intervalo de salvamento automático", en: "Autosave interval" },
  minutes: { "pt-BR": "minutos", en: "minutes" },
  removeRecent: { "pt-BR": "Remover da lista", en: "Remove from list" },
  recoverAutosave: { "pt-BR": "Recuperar salvamento automático", en: "Recover autosave" },
  discardAutosave: { "pt-BR": "Descartar salvamento automático", en: "Discard autosave" },
  unsavedConfirm: { "pt-BR": "Há alterações não salvas. Deseja continuar?", en: "You have unsaved changes. Continue?" },
  exportSrt: { "pt-BR": "Exportar SRT", en: "Export SRT" },
  gpuEncoder: { "pt-BR": "Codificador GPU", en: "GPU encoder" },
  matchLoudness: { "pt-BR": "Igualar loudness", en: "Match loudness" },
  pauseExport: { "pt-BR": "Pausar", en: "Pause" },
  resumeExport: { "pt-BR": "Retomar", en: "Resume" },
  removeExport: { "pt-BR": "Remover", en: "Remove" },
  elapsed: { "pt-BR": "decorrido", en: "elapsed" },
} as const;

export type TextKey = keyof typeof strings;

export function detectLocale(): Locale {
  return navigator.language.toLowerCase().startsWith("pt") ? "pt-BR" : "en";
}

let current: Locale = typeof navigator === "undefined" ? "en" : detectLocale();
const localeListeners = new Set<() => void>();

export function setLocale(locale: Locale) {
  if (current === locale) return;
  current = locale;
  localeListeners.forEach((listener) => listener());
}

export function localeFromPrefs(locale: string): Locale {
  return locale === "en" ? "en" : "pt-BR";
}

export function subscribeLocale(listener: () => void) {
  localeListeners.add(listener);
  return () => localeListeners.delete(listener);
}

export function localeSnapshot() {
  return current;
}

export function t(key: TextKey): string {
  return strings[key][current];
}

/** `mm:ss.hh` for timeline/transport readouts (hundredths, not frames: fps varies per clip). */
export function formatTime(secs: number): string {
  const totalHundredths = Math.round(Math.max(0, secs) * 100);
  const minutes = Math.floor(totalHundredths / 6000);
  const seconds = Math.floor((totalHundredths % 6000) / 100);
  const hundredths = totalHundredths % 100;
  const pad = (n: number) => n.toString().padStart(2, "0");
  return `${pad(minutes)}:${pad(seconds)}.${pad(hundredths)}`;
}
