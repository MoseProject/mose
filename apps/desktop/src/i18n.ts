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
  colorAdjust: { "pt-BR": "Ajuste de cor", en: "Color adjust" },
  keyframes: { "pt-BR": "Keyframes", en: "Keyframes" },
  speedRamp: { "pt-BR": "Rampa de velocidade", en: "Speed ramp" },
  speedRampSlowToFast: {
    "pt-BR": "Lento → Rápido (0.5x → 2x)",
    en: "Slow → Fast (0.5x → 2x)",
  },
  speedRampFastToSlow: {
    "pt-BR": "Rápido → Lento (2x → 0.5x)",
    en: "Fast → Slow (2x → 0.5x)",
  },
  layerTransformDragHint: {
    "pt-BR": "Arraste para posicionar a camada — efeito visível só em faixas de overlay.",
    en: "Drag to position the layer — only visible on overlay tracks.",
  },
  layerTransformResizeHint: {
    "pt-BR": "Arraste para redimensionar a camada — efeito visível só em faixas de overlay.",
    en: "Drag to resize the layer — only visible on overlay tracks.",
  },
  layerTransformAnimatedHint: {
    "pt-BR": "Posição animada — edite pela lista de keyframes.",
    en: "Position is animated — edit via the keyframe list.",
  },
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
  drawCustomShape: { "pt-BR": "Desenhar forma", en: "Draw shape" },
  drawCustomShapeHint: {
    "pt-BR": "Clique para adicionar vértices, Enter para finalizar (mín. 3), Esc para cancelar.",
    en: "Click to add vertices, Enter to finish (3+ needed), Esc to cancel.",
  },
  addTextTrack: { "pt-BR": "Adicionar faixa de texto", en: "Add text track" },
  addShapeTrack: { "pt-BR": "Adicionar faixa de forma", en: "Add shape track" },
  hideTrack: { "pt-BR": "Ocultar faixa", en: "Hide track" },
  showTrack: { "pt-BR": "Mostrar faixa", en: "Show track" },
  lockTrack: { "pt-BR": "Travar faixa", en: "Lock track" },
  unlockTrack: { "pt-BR": "Destravar faixa", en: "Unlock track" },
  zoomIn: { "pt-BR": "Aumentar zoom", en: "Zoom in" },
  zoomOut: { "pt-BR": "Diminuir zoom", en: "Zoom out" },
  toolSelect: { "pt-BR": "Selecionar", en: "Select" },
  toolRoll: { "pt-BR": "Rolar", en: "Roll" },
  toolSlip: { "pt-BR": "Deslizar origem", en: "Slip" },
  toolSlide: { "pt-BR": "Deslizar", en: "Slide" },
  toolText: { "pt-BR": "Texto (T)", en: "Text (T)" },
  textToolHint: {
    "pt-BR": "Clique na pré-visualização para criar um texto nessa posição.",
    en: "Click the preview to create a text graphic at that position.",
  },
  mergeComposite: { "pt-BR": "Mesclar em bloco composto", en: "Merge into composite block" },
  compoundClip: { "pt-BR": "Criar clipe composto", en: "Create compound clip" },
  insertAsCompoundClip: {
    "pt-BR": "Inserir como clipe composto",
    en: "Insert as compound clip",
  },
  templates: { "pt-BR": "Templates", en: "Templates" },
  saveTemplateTitle: { "pt-BR": "Salvar template", en: "Save template" },
  saveTemplateConfirm: { "pt-BR": "Salvar", en: "Save" },
  noSavedTemplates: { "pt-BR": "Nenhum template salvo ainda.", en: "No saved templates yet." },
  applyTemplate: { "pt-BR": "Aplicar", en: "Apply" },
  deleteTemplate: { "pt-BR": "🗑", en: "🗑" },
  applyTemplateTitle: { "pt-BR": "Aplicar template", en: "Apply template" },
  applyTemplateLayerLabel: { "pt-BR": "Camada", en: "Layer" },
  applyTemplatePickAsset: { "pt-BR": "Escolha um arquivo…", en: "Pick a file…" },
  applyTemplateConfirm: { "pt-BR": "Criar camadas", en: "Create layers" },
  saveAsTemplate: { "pt-BR": "Salvar como template", en: "Save as template" },

  keyframeTime: { "pt-BR": "Tempo", en: "Time" },
  keyframeX: { "pt-BR": "X", en: "X" },
  keyframeY: { "pt-BR": "Y", en: "Y" },
  addKeyframe: { "pt-BR": "+ Keyframe", en: "+ Keyframe" },
  crop: { "pt-BR": "Corte", en: "Crop" },
  cropReset: { "pt-BR": "Redefinir corte", en: "Reset crop" },
  mask: { "pt-BR": "Máscara", en: "Mask" },
  maskCornerRadius: { "pt-BR": "Raio do canto", en: "Corner radius" },
  maskNone: { "pt-BR": "Nenhuma", en: "None" },
  maskCircle: { "pt-BR": "Círculo", en: "Circle" },
  maskRoundedRect: { "pt-BR": "Retângulo arredondado", en: "Rounded rectangle" },
  transition: { "pt-BR": "Transição", en: "Transition" },
  transitionDuration: { "pt-BR": "Duração da transição", en: "Transition duration" },
  transitionNone: { "pt-BR": "Nenhuma", en: "None" },
  transitionFade: { "pt-BR": "Fade", en: "Fade" },
  transitionHardCut: { "pt-BR": "Corte seco", en: "Hard cut" },
  transitionSlide: { "pt-BR": "Deslizar", en: "Slide" },
  transitionZoom: { "pt-BR": "Zoom", en: "Zoom" },
  blendMode: { "pt-BR": "Modo de mesclagem", en: "Blend mode" },
  positionKeyframes: { "pt-BR": "Keyframes de posição", en: "Position keyframes" },
  scaleKeyframes: { "pt-BR": "Keyframes de escala", en: "Scale keyframes" },
  rotationKeyframes: { "pt-BR": "Keyframes de rotação", en: "Rotation keyframes" },
  opacityKeyframes: { "pt-BR": "Keyframes de opacidade", en: "Opacity keyframes" },
  posXKeyframes: { "pt-BR": "Posição X (keyframes)", en: "Position X (keyframes)" },
  posYKeyframes: { "pt-BR": "Posição Y (keyframes)", en: "Position Y (keyframes)" },
  widthKeyframes: { "pt-BR": "Largura (keyframes)", en: "Width (keyframes)" },
  heightKeyframes: { "pt-BR": "Altura (keyframes)", en: "Height (keyframes)" },
  fontWeight: { "pt-BR": "Peso da fonte", en: "Font weight" },
  direction: { "pt-BR": "Direção do texto", en: "Text direction" },
  directionAuto: { "pt-BR": "Automática", en: "Auto" },
  directionLtr: { "pt-BR": "Esquerda para direita", en: "Left to right" },
  directionRtl: { "pt-BR": "Direita para esquerda", en: "Right to left" },
  backgroundEnabled: { "pt-BR": "Fundo", en: "Background" },
  backgroundColor: { "pt-BR": "Cor do fundo", en: "Background color" },
  backgroundPadding: { "pt-BR": "Espaçamento", en: "Padding" },
  backgroundRadius: { "pt-BR": "Cantos arredondados", en: "Corner radius" },
  overlayStart: { "pt-BR": "Início (s)", en: "Start (s)" },
  overlayDuration: { "pt-BR": "Duração (s)", en: "Duration (s)" },
  shapeRotation: { "pt-BR": "Rotação", en: "Rotation" },
  shapeStroke: { "pt-BR": "Espessura do contorno (px)", en: "Outline thickness (px)" },
  shapeStrokeHint: { "pt-BR": "0 = preenchido", en: "0 = filled" },
  shapeVertices: { "pt-BR": "Vértices", en: "Vertices" },
  shapeVerticesHint: {
    "pt-BR":
      "arraste os valores X/Y para desenhar uma forma personalizada — funciona em qualquer predefinição, já que todas são polígonos por baixo dos panos (exceto elipse/círculo).",
    en: "drag the X/Y values to draw a custom shape — works on any preset, since they're all polygons underneath (except ellipse/circle).",
  },
  gainKeyframes: { "pt-BR": "Keyframes de ganho", en: "Gain keyframes" },
  brightnessKeyframes: { "pt-BR": "Keyframes de brilho", en: "Brightness keyframes" },
  contrastKeyframes: { "pt-BR": "Keyframes de contraste", en: "Contrast keyframes" },
  saturationKeyframes: { "pt-BR": "Keyframes de saturação", en: "Saturation keyframes" },
  cropXKeyframes: { "pt-BR": "Keyframes de corte X", en: "Crop X keyframes" },
  cropYKeyframes: { "pt-BR": "Keyframes de corte Y", en: "Crop Y keyframes" },
  cropWKeyframes: { "pt-BR": "Keyframes de largura do corte", en: "Crop width keyframes" },
  cropHKeyframes: { "pt-BR": "Keyframes de altura do corte", en: "Crop height keyframes" },

  transcribe: { "pt-BR": "Transcrever", en: "Transcribe" },
  transcribing: { "pt-BR": "Transcrevendo…", en: "Transcribing…" },
  transcribeNoSpeechFound: { "pt-BR": "Nenhuma fala encontrada.", en: "No speech found." },
  transcriptPanelTitle: { "pt-BR": "Transcrição", en: "Transcript" },
  transcriptPanelSearchHint: {
    "pt-BR": "Buscar nesta transcrição…",
    en: "Search this transcript…",
  },
  transcriptProjectSearchHint: { "pt-BR": "Buscar no projeto…", en: "Search the project…" },
  transcriptSearchProject: { "pt-BR": "Buscar no projeto", en: "Search project" },
  transcriptProjectEmpty: { "pt-BR": "Nenhum resultado.", en: "No results." },
  transcriptPanelEmpty: { "pt-BR": "Nenhuma palavra encontrada.", en: "No words found." },
  transcriptPanelNoTranscript: {
    "pt-BR": 'Este clipe ainda não tem transcrição. Use "Transcrever" na biblioteca de mídia.',
    en: 'This clip has no transcript yet. Use "Transcribe" in the media library.',
  },
  transcriptPanelNoClip: {
    "pt-BR": "Selecione um clipe de vídeo ou áudio.",
    en: "Select a video or audio clip.",
  },
  whisperModelPath: { "pt-BR": "Modelo Whisper (.bin)", en: "Whisper model (.bin)" },
  reframeModelPath: { "pt-BR": "Modelo de reenquadramento (.onnx)", en: "Reframe model (.onnx)" },
  backgroundRemovalModelPath: {
    "pt-BR": "Modelo de remoção de fundo (.onnx)",
    en: "Background removal model (.onnx)",
  },
  ttsModelPath: { "pt-BR": "Modelo de voz (.onnx)", en: "Voice model (.onnx)" },
  soundLibraryPath: { "pt-BR": "Pasta da biblioteca de som", en: "Sound library folder" },
  soundLibrary: { "pt-BR": "Biblioteca de som", en: "Sound library" },
  soundLibraryEmpty: {
    "pt-BR": "Configure a pasta da biblioteca de som nas preferências.",
    en: "Configure the sound library folder in preferences.",
  },

  watchFolder: { "pt-BR": "Monitorar pasta", en: "Watch folder" },
  watchFolderChoose: { "pt-BR": "Escolher pasta…", en: "Choose folder…" },
  watchFolderStart: { "pt-BR": "Iniciar monitoramento", en: "Start watching" },
  watchFolderStop: { "pt-BR": "Parar monitoramento", en: "Stop watching" },
  watchFolderEmpty: {
    "pt-BR": "Nenhum arquivo detectado ainda.",
    en: "No files detected yet.",
  },
  watchFolderAddToProject: { "pt-BR": "Adicionar ao projeto", en: "Add to project" },
  watchFolderStabilizing: {
    "pt-BR": "Aguardando gravação terminar…",
    en: "Waiting for recording to finish…",
  },
  watchFolderProcessing: { "pt-BR": "Processando…", en: "Processing…" },
  watchFolderDone: { "pt-BR": "Concluído", en: "Done" },
  watchFolderError: { "pt-BR": "Erro", en: "Error" },

  youtubeDownload: { "pt-BR": "Baixar do YouTube", en: "Download from YouTube" },
  youtubeDownloadUrlHint: { "pt-BR": "Cole o link do vídeo…", en: "Paste the video link…" },
  youtubeDownloadStart: { "pt-BR": "Baixar", en: "Download" },
  youtubeDownloadCancel: { "pt-BR": "Cancelar download", en: "Cancel download" },
  youtubeDownloadInProgress: { "pt-BR": "Baixando…", en: "Downloading…" },
  youtubeDownloadToolMissing: {
    "pt-BR": "yt-dlp não está instalado.",
    en: "yt-dlp is not installed.",
  },

  crashReviewTitle: { "pt-BR": "O oca fechou inesperadamente", en: "oca closed unexpectedly" },
  crashReviewOffer: {
    "pt-BR":
      "O oca fechou inesperadamente na última vez. Você quer enviar um relatório para ajudar a corrigir isso?",
    en: "oca closed unexpectedly last time. Would you like to send a report to help fix this?",
  },
  crashReviewShowPayload: { "pt-BR": "Ver dados do relatório", en: "Show report payload" },
  crashReviewSendOnce: { "pt-BR": "Enviar uma vez", en: "Send once" },
  crashReviewAlwaysSend: { "pt-BR": "Sempre enviar relatórios", en: "Always send error reports" },
  crashReviewDoNotSend: { "pt-BR": "Não enviar", en: "Do not send" },

  telemetryEnabled: {
    "pt-BR": "Telemetria local (nunca enviada)",
    en: "Local telemetry (never sent)",
  },
  errorReportingConsent: {
    "pt-BR": "Sempre enviar relatórios de erro",
    en: "Always send error reports",
  },
  queuedReports: { "pt-BR": "relatórios na fila", en: "queued reports" },
  deleteQueuedReports: { "pt-BR": "Excluir relatórios na fila", en: "Delete queued reports" },

  checkForUpdate: { "pt-BR": "Verificar atualizações", en: "Check for updates" },
  updateCheckUpToDate: {
    "pt-BR": "Você está usando a versão mais recente.",
    en: "You're up to date.",
  },
  updateCheckFailed: {
    "pt-BR": "Falha ao verificar atualizações.",
    en: "Failed to check for updates.",
  },
  updateAvailable: { "pt-BR": "Nova versão disponível", en: "New version available" },
  updateInstall: { "pt-BR": "Instalar", en: "Install" },
  updateOpenRelease: { "pt-BR": "Abrir página de lançamento", en: "Open release page" },
  updateInstalled: {
    "pt-BR": "Atualização instalada. Reinicie para aplicar.",
    en: "Update installed. Restart to apply.",
  },
  updateInstallFailed: {
    "pt-BR": "Falha ao instalar atualização.",
    en: "Failed to install update.",
  },
  updateRestart: { "pt-BR": "Reiniciar", en: "Restart" },

  silenceReview: { "pt-BR": "Revisar silêncio", en: "Review silence" },
  silenceReviewTitle: { "pt-BR": "Revisar silêncio", en: "Review silence" },
  silenceReviewEmpty: { "pt-BR": "Nenhum silêncio encontrado.", en: "No silence found." },
  silenceReviewApply: { "pt-BR": "Aplicar", en: "Apply" },
  silenceReviewSelectClipFirst: {
    "pt-BR": "Selecione um clipe primeiro.",
    en: "Select a clip first.",
  },

  detectChapters: { "pt-BR": "🎬 Detectar capítulos", en: "🎬 Detect Chapters" },
  detectHighlightsAction: { "pt-BR": "Detectar destaques", en: "Detect Highlights" },
  sceneCutDetectionNone: {
    "pt-BR": "Nenhum corte de cena encontrado.",
    en: "No scene cuts found.",
  },
  motionTrackNoFramesDecoded: {
    "pt-BR": "Não foi possível decodificar quadros para rastrear.",
    en: "Couldn't decode frames to track.",
  },
  trackMotion: { "pt-BR": "Rastrear movimento", en: "Track motion" },
  autoReframe: { "pt-BR": "Reenquadramento automático", en: "Auto reframe" },
  dynamicAutoReframe: { "pt-BR": "Reenquadramento dinâmico", en: "Dynamic auto reframe" },
  autoReframeNoSubjectFound: {
    "pt-BR": "Nenhum sujeito encontrado; usando corte centralizado.",
    en: "No subject found; using a centered crop.",
  },
  generateMatte: { "pt-BR": "Gerar máscara", en: "Generate matte" },
  applyPrivacyBlur: { "pt-BR": "Aplicar blur", en: "Apply blur" },
  centerX: { "pt-BR": "Centro X", en: "Center X" },
  centerY: { "pt-BR": "Centro Y", en: "Center Y" },
  regionWidth: { "pt-BR": "Largura", en: "Width" },
  regionHeight: { "pt-BR": "Altura", en: "Height" },
  aiActions: { "pt-BR": "Ações de IA", en: "AI actions" },
  ttsButton: { "pt-BR": "Texto-pra-fala", en: "Text to speech" },
  ttsModalTitle: { "pt-BR": "Texto-pra-fala", en: "Text to speech" },
  ttsGenerate: { "pt-BR": "Gerar", en: "Generate" },
  ttsPlaceholder: { "pt-BR": "Digite o texto…", en: "Type the text…" },
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
  watchFolderNone: { "pt-BR": "Nenhuma pasta selecionada", en: "No folder selected" },
  tools: { "pt-BR": "Ferramentas", en: "Tools" },
  timecode: { "pt-BR": "Posição do cursor", en: "Playhead position" },
  seek: { "pt-BR": "Posição de reprodução", en: "Playback position" },
  zoomFit: { "pt-BR": "Ajustar a timeline à janela", en: "Fit timeline to window" },
  removeVertex: { "pt-BR": "Remover vértice", en: "Remove vertex" },
  goToGap: { "pt-BR": "Ir para o trecho", en: "Go to gap" },
  menuFile: { "pt-BR": "Arquivo", en: "File" },
  menuEdit: { "pt-BR": "Editar", en: "Edit" },
  menuView: { "pt-BR": "Exibir", en: "View" },
  menuSequence: { "pt-BR": "Sequência", en: "Sequence" },
  menuGraphics: { "pt-BR": "Gráficos", en: "Graphics" },
  menuCopy: { "pt-BR": "Copiar", en: "Copy" },
  menuCut: { "pt-BR": "Recortar", en: "Cut" },
  menuPaste: { "pt-BR": "Colar", en: "Paste" },
  projectLabel: { "pt-BR": "Projeto", en: "Project" },
  programMonitor: { "pt-BR": "MONITOR DE PROGRAMA", en: "PROGRAM MONITOR" },
  meterTarget: { "pt-BR": "alvo", en: "target" },
  meterLoudness: { "pt-BR": "Loudness momentâneo", en: "Momentary loudness" },
  audioMeter: { "pt-BR": "Medidor de áudio", en: "Audio meter" },
  goToStart: { "pt-BR": "Ir para o início", en: "Go to start" },
  goToEnd: { "pt-BR": "Ir para o fim", en: "Go to end" },
  libraryTabMedia: { "pt-BR": "MÍDIA", en: "MEDIA" },
  libraryTabFavorites: { "pt-BR": "FAVORITOS", en: "FAVORITES" },
  libraryTabRecent: { "pt-BR": "RECENTES", en: "RECENT" },
  searchMedia: { "pt-BR": "Buscar mídia…", en: "Search media…" },
  viewGrid: { "pt-BR": "Exibir em grade", en: "Grid view" },
  viewList: { "pt-BR": "Exibir em lista", en: "List view" },
  projectMedia: { "pt-BR": "Mídia do projeto", en: "Project media" },
  items: { "pt-BR": "itens", en: "items" },
  libraryNoMatches: { "pt-BR": "Nada encontrado.", en: "Nothing found." },
  inspectorEffects: { "pt-BR": "EFEITOS", en: "EFFECTS" },
  inspectorAudio: { "pt-BR": "ÁUDIO", en: "AUDIO" },
  exportDialogTitle: { "pt-BR": "Exportar sequência", en: "Export sequence" },
  exportChooseFile: { "pt-BR": "Escolher arquivo e exportar", en: "Choose file and export" },
  loudnessTarget: { "pt-BR": "Loudness alvo", en: "Loudness target" },
  loudnessCustom: { "pt-BR": "Personalizado", en: "Custom" },
  loudnessRange: {
    "pt-BR": "Use um valor entre -40 e -5 LUFS.",
    en: "Use a value between -40 and -5 LUFS.",
  },
  loudnessMeasured: { "pt-BR": "Medido no primeiro clipe", en: "Measured on the first clip" },
  exportKeepsBitrate: {
    "pt-BR": "O vídeo é exportado com o bitrate da fonte.",
    en: "Video is exported at the source bitrate.",
  },
  groupPlayback: { "pt-BR": "Reprodução", en: "Playback" },
  groupEdit: { "pt-BR": "Edição", en: "Editing" },
  groupTimeline: { "pt-BR": "Timeline", en: "Timeline" },
  groupTools: { "pt-BR": "Ferramentas", en: "Tools" },
  groupFile: { "pt-BR": "Arquivo", en: "File" },
  groupView: { "pt-BR": "Exibição", en: "View" },
  cmdPlayPause: { "pt-BR": "Reproduzir / pausar", en: "Play / pause" },
  cmdPlay: { "pt-BR": "Reproduzir (repetir acelera)", en: "Play (repeat to speed up)" },
  cmdShuttleBack: {
    "pt-BR": "Voltar (repetir acelera)",
    en: "Reverse (repeat to speed up)",
  },
  cmdPause: { "pt-BR": "Pausar", en: "Pause" },
  shuttleBadge: { "pt-BR": "Shuttle", en: "Shuttle" },
  cmdBack1s: { "pt-BR": "Voltar 1 segundo", en: "Back 1 second" },
  cmdForward1s: { "pt-BR": "Avançar 1 segundo", en: "Forward 1 second" },
  cmdBack10s: { "pt-BR": "Voltar 10 segundos", en: "Back 10 seconds" },
  cmdForward10s: { "pt-BR": "Avançar 10 segundos", en: "Forward 10 seconds" },
  cmdStepBack10: { "pt-BR": "Voltar 10 quadros", en: "Back 10 frames" },
  cmdStepForward10: { "pt-BR": "Avançar 10 quadros", en: "Forward 10 frames" },
  cmdPrevEdit: { "pt-BR": "Ponto de corte anterior", en: "Previous cut point" },
  cmdNextEdit: { "pt-BR": "Próximo ponto de corte", en: "Next cut point" },
  cmdMarkIn: { "pt-BR": "Marcar entrada", en: "Mark in" },
  cmdMarkOut: { "pt-BR": "Marcar saída", en: "Mark out" },
  cmdClearMarks: { "pt-BR": "Limpar entrada e saída", en: "Clear in and out" },
  cmdDeleteRange: {
    "pt-BR": "Excluir trecho entre entrada e saída",
    en: "Delete the range between in and out",
  },
  cmdLiftRange: {
    "pt-BR": "Remover trecho entre entrada e saída (deixa o vão)",
    en: "Lift the range between in and out (leaves the gap)",
  },
  cmdGoToMarkIn: { "pt-BR": "Ir para a entrada", en: "Go to in" },
  cmdGoToMarkOut: { "pt-BR": "Ir para a saída", en: "Go to out" },
  cmdPlayRange: { "pt-BR": "Reproduzir de entrada a saída", en: "Play from in to out" },
  cmdSelectAll: { "pt-BR": "Selecionar tudo", en: "Select all" },
  cmdDeselect: { "pt-BR": "Limpar seleção", en: "Clear selection" },
  cmdDuplicate: { "pt-BR": "Duplicar clipe", en: "Duplicate clip" },
  cmdCopyFormat: { "pt-BR": "Copiar formatação", en: "Copy formatting" },
  cmdPasteFormat: { "pt-BR": "Colar formatação", en: "Paste formatting" },
  cmdRenameSequence: { "pt-BR": "Renomear sequência", en: "Rename sequence" },
  cmdToggleSnap: { "pt-BR": "Ligar ou desligar o ímã", en: "Toggle snapping" },
  cmdShortcuts: { "pt-BR": "Atalhos de teclado", en: "Keyboard shortcuts" },
  cmdPalette: { "pt-BR": "Paleta de comandos", en: "Command palette" },
  cmdSettings: { "pt-BR": "Ajustes", en: "Settings" },
  cmdScrollToPlayhead: { "pt-BR": "Mostrar o cursor na timeline", en: "Show the playhead" },
  cmdSplit: { "pt-BR": "Dividir no cursor", en: "Split at playhead" },
  cmdToolSelect: { "pt-BR": "Ferramenta Selecionar", en: "Select tool" },
  cmdToolRoll: { "pt-BR": "Ferramenta Rolar", en: "Roll tool" },
  cmdToolSlip: { "pt-BR": "Ferramenta Deslizar origem", en: "Slip tool" },
  cmdToolSlide: { "pt-BR": "Ferramenta Deslizar", en: "Slide tool" },
  cmdToolText: { "pt-BR": "Ferramenta Texto", en: "Text tool" },
  cmdToggleGuides: { "pt-BR": "Mostrar guias no monitor", en: "Show monitor guides" },
  shortcutsSearch: { "pt-BR": "Buscar atalho…", en: "Search shortcuts…" },
  shortcutsNone: { "pt-BR": "Nenhum atalho encontrado.", en: "No shortcuts found." },
  paletteSearch: { "pt-BR": "Digite um comando…", en: "Type a command…" },
  paletteNone: { "pt-BR": "Nenhum comando encontrado.", en: "No commands found." },
  keySpace: { "pt-BR": "Espaço", en: "Space" },
  muteTrack: { "pt-BR": "Silenciar faixa", en: "Mute track" },
  unmuteTrack: { "pt-BR": "Reativar o som da faixa", en: "Unmute track" },
  soloTrack: { "pt-BR": "Ouvir só esta faixa", en: "Solo track" },
  unsoloTrack: { "pt-BR": "Desfazer o solo", en: "Clear solo" },
  audioRoleLabel: { "pt-BR": "Papel do áudio", en: "Audio role" },
  roleUnspecified: { "pt-BR": "Sem papel", en: "No role" },
  roleGame: { "pt-BR": "Jogo", en: "Game" },
  roleMic: { "pt-BR": "Microfone", en: "Mic" },
  roleMusic: { "pt-BR": "Música", en: "Music" },
  timelineCanvas: {
    "pt-BR": "Timeline. Use as setas para trocar de clipe e Enter para ir até ele.",
    en: "Timeline. Use the arrow keys to change clip and Enter to go to it.",
  },
  timelineClips: { "pt-BR": "Clipes da timeline", en: "Timeline clips" },
  timelineOverview: { "pt-BR": "Visão geral da timeline", en: "Timeline overview" },
  notifications: { "pt-BR": "Notificações", en: "Notifications" },
  confirmOverwriteTitle: { "pt-BR": "Substituir o arquivo?", en: "Replace the file?" },
  confirmOverwriteAction: { "pt-BR": "Substituir", en: "Replace" },
  confirmKeepBoth: { "pt-BR": "Salvar com outro nome", en: "Save under another name" },
  etaRemaining: { "pt-BR": "restantes", en: "left" },
  openExportFolder: { "pt-BR": "Mostrar na pasta", en: "Show in folder" },
  exportOutputIsSource: {
    "pt-BR": "Esse arquivo é uma mídia do projeto. Escolha outro nome para a exportação.",
    en: "That file is a media file of the project. Pick another name for the export.",
  },
  exportOutputInQueue: {
    "pt-BR": "Já existe uma exportação para esse arquivo na fila.",
    en: "An export to that file is already in the queue.",
  },
  importFailed: {
    "pt-BR": "Não foi possível importar o arquivo",
    en: "Could not import the file",
  },
  removeFromLibrary: { "pt-BR": "Remover da biblioteca", en: "Remove from library" },
  removeFromLibraryTitle: { "pt-BR": "Remover da biblioteca?", en: "Remove from library?" },
  removeFromLibraryMessage: {
    "pt-BR": "O arquivo original não é apagado. Só sai desta biblioteca.",
    en: "The original file is not deleted. It only leaves this library.",
  },
  assetInUse: {
    "pt-BR": "Esta mídia está em uso na timeline. Remova os clipes antes.",
    en: "This media is used on the timeline. Remove its clips first.",
  },
  confirmDiscardTitle: { "pt-BR": "Descartar alterações?", en: "Discard changes?" },
  confirmDiscardAction: { "pt-BR": "Descartar", en: "Discard" },
  taskAnalyzing: { "pt-BR": "Analisando", en: "Analyzing" },
  tasks: { "pt-BR": "Tarefas em andamento", en: "Running tasks" },
  settingsGeneral: { "pt-BR": "Geral", en: "General" },
  settingsExport: { "pt-BR": "Exportação", en: "Export" },
  settingsModels: { "pt-BR": "IA e modelos", en: "AI and models" },
  settingsPrivacy: { "pt-BR": "Privacidade", en: "Privacy" },
  settingsUpdates: { "pt-BR": "Atualizações", en: "Updates" },
  browse: { "pt-BR": "Procurar…", en: "Browse…" },
  pathOk: { "pt-BR": "Encontrado", en: "Found" },
  pathMissing: { "pt-BR": "Não encontrado", en: "Not found" },
  pathWrongKind: { "pt-BR": "Tipo de caminho inesperado", en: "Unexpected kind of path" },
  settingsSaved: { "pt-BR": "Salvo", en: "Saved" },
  trimTools: { "pt-BR": "Ferramentas de aparo", en: "Trim tools" },
  monitorPanHint: {
    "pt-BR": "Arraste para mover a imagem. Clique duplo centraliza.",
    en: "Drag to move the image. Double-click to center it.",
  },
  monitorZoom: { "pt-BR": "Zoom do monitor", en: "Monitor zoom" },
  zoomFitShort: { "pt-BR": "Ajustar", en: "Fit" },
  moreSources: { "pt-BR": "Mais fontes de mídia", en: "More media sources" },
  transform: { "pt-BR": "Transformar", en: "Transform" },
  scale: { "pt-BR": "Escala", en: "Scale" },
  rotation: { "pt-BR": "Rotação", en: "Rotation" },
  transformAnimated: {
    "pt-BR": "Valor animado: edite nos keyframes.",
    en: "Animated value: edit it in the keyframes.",
  },
  transformHint: {
    "pt-BR": "Também dá para arrastar a camada no monitor.",
    en: "You can also drag the layer in the monitor.",
  },
  textTypography: { "pt-BR": "Tipografia", en: "Typography" },
  textBackground: { "pt-BR": "Fundo", en: "Background" },
  textPosition: { "pt-BR": "Posição", en: "Position" },
  textHighlight: { "pt-BR": "Destaque", en: "Highlight" },
  textTiming: { "pt-BR": "Tempo", en: "Timing" },
  keyframeToggle: { "pt-BR": "Keyframe no cursor", en: "Keyframe at the playhead" },
  keyframeOutside: {
    "pt-BR": "Posicione o cursor dentro do clipe para criar um keyframe.",
    en: "Move the playhead inside the clip to add a keyframe.",
  },
  resetSection: { "pt-BR": "Restaurar padrão", en: "Reset to default" },
  clipsSelected: { "pt-BR": "clipes selecionados", en: "clips selected" },
  keymapReserved: {
    "pt-BR": "Esse atalho é reservado pelo sistema ou pelos campos de texto.",
    en: "That shortcut is reserved by the system or by text fields.",
  },
  keymapDuplicate: {
    "pt-BR": "Esse comando já usa esse atalho.",
    en: "That command already uses this shortcut.",
  },
  keymapFull: {
    "pt-BR": "Cada comando aceita no máximo 4 atalhos.",
    en: "Each command takes at most 4 shortcuts.",
  },
  keymapUsedBy: { "pt-BR": "já é usado por", en: "is already used by" },
  keymapReplace: { "pt-BR": "Substituir", en: "Replace" },
  keymapUnbound: { "pt-BR": "Sem atalho", en: "No shortcut" },
  keymapRemove: { "pt-BR": "Remover atalho", en: "Remove shortcut" },
  keymapAdd: { "pt-BR": "Adicionar atalho", en: "Add shortcut" },
  keymapReset: { "pt-BR": "Restaurar o atalho padrão", en: "Restore the default shortcut" },
  keymapPress: {
    "pt-BR": "Pressione as teclas… (Esc cancela)",
    en: "Press the keys… (Esc cancels)",
  },
  keymapResetAll: { "pt-BR": "Restaurar todos os padrões", en: "Restore all defaults" },
  keymapResetAllTitle: {
    "pt-BR": "Restaurar os atalhos padrão?",
    en: "Restore the default shortcuts?",
  },
  keymapResetAllMessage: {
    "pt-BR": "Todos os atalhos que você alterou voltam ao padrão. Isso não pode ser desfeito.",
    en: "Every shortcut you changed goes back to its default. This cannot be undone.",
  },
  close: { "pt-BR": "Fechar", en: "Close" },
  menuAdd: { "pt-BR": "Adicionar", en: "Add" },
  menuClip: { "pt-BR": "Clipe", en: "Clip" },
  menuAnalyze: { "pt-BR": "Analisar", en: "Analyze" },
  sequenceActions: { "pt-BR": "Ações da sequência", en: "Sequence actions" },
  gpuEncoderHint: {
    "pt-BR": "Usado em todas as exportações. Auto escolhe o melhor disponível.",
    en: "Used for every export. Auto picks the best available.",
  },
  exportOverwriteConfirm: {
    "pt-BR": "O arquivo já existe. Substituir? (Cancelar cria uma cópia com novo nome.)",
    en: "The file already exists. Overwrite it? (Cancel creates a renamed copy.)",
  },
  noExports: { "pt-BR": "Nenhuma exportação nesta sessão.", en: "No exports this session." },
  errorPrefix: { "pt-BR": "Erro", en: "Error" },
  preferences: { "pt-BR": "Ajustes", en: "Preferences" },
  language: { "pt-BR": "Idioma", en: "Language" },
  defaultLufs: { "pt-BR": "Perfil LUFS padrão", en: "Default LUFS profile" },
  outputFolder: { "pt-BR": "Pasta de saída", en: "Output folder" },
  autosaveInterval: { "pt-BR": "Intervalo de salvamento automático", en: "Autosave interval" },
  minutes: { "pt-BR": "minutos", en: "minutes" },
  removeRecent: { "pt-BR": "Remover da lista", en: "Remove from list" },
  recoverAutosave: { "pt-BR": "Recuperar salvamento automático", en: "Recover autosave" },
  discardAutosave: { "pt-BR": "Descartar salvamento automático", en: "Discard autosave" },
  unsavedConfirm: {
    "pt-BR": "Há alterações não salvas. Deseja continuar?",
    en: "You have unsaved changes. Continue?",
  },
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
  if (current === locale) {
    return;
  }
  current = locale;
  if (typeof document !== "undefined") {
    document.documentElement.lang = locale;
  }
  localeListeners.forEach((listener) => {
    return listener();
  });
}

export function localeFromPrefs(locale: string): Locale {
  return locale === "en" ? "en" : "pt-BR";
}

export function subscribeLocale(listener: () => void) {
  localeListeners.add(listener);
  return () => {
    return localeListeners.delete(listener);
  };
}

export function localeSnapshot() {
  return current;
}

export function t(key: TextKey): string {
  return strings[key][current];
}

/** `mm:ss.hh` for timeline/transport readouts (hundredths, not frames: fps varies per clip). */
/** `HH:MM:SS:FF` at `fps` frames per second (the usual editing timecode). */
export function formatTimecode(secs: number, fps: number): string {
  const rate = fps > 0 ? Math.round(fps) : 30;
  const totalFrames = Math.round(Math.max(0, secs) * rate);
  const frames = totalFrames % rate;
  const totalSecs = Math.floor(totalFrames / rate);
  const pad = (n: number) => {
    return n.toString().padStart(2, "0");
  };
  return [
    pad(Math.floor(totalSecs / 3600)),
    pad(Math.floor((totalSecs % 3600) / 60)),
    pad(totalSecs % 60),
    pad(frames),
  ].join(":");
}

export function formatTime(secs: number): string {
  const totalHundredths = Math.round(Math.max(0, secs) * 100);
  const minutes = Math.floor(totalHundredths / 6000);
  const seconds = Math.floor((totalHundredths % 6000) / 100);
  const hundredths = totalHundredths % 100;
  const pad = (n: number) => {
    return n.toString().padStart(2, "0");
  };
  return `${pad(minutes)}:${pad(seconds)}.${pad(hundredths)}`;
}
