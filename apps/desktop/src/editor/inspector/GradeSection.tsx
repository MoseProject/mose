import { t, type TextKey } from "../../i18n";
import { MAX_GRADE_EXPOSURE, NEUTRAL_GRADE, type ClipInstance, type ClipPatch } from "../../types";
import type { Grade } from "../../types";
import { Section, Slider } from "./controls";

type Commit = (patch: ClipPatch) => Promise<unknown>;

const format = (v: number) => {
  return v.toFixed(2);
};

const CHANNELS: [number, TextKey][] = [
  [0, "gradeRed"],
  [1, "gradeGreen"],
  [2, "gradeBlue"],
];

/** Whether the grade differs from neutral. */
export function gradeIsNeutral(grade: Grade): boolean {
  return JSON.stringify(grade) === JSON.stringify(NEUTRAL_GRADE);
}

/** The colour grade: exposure, temperature and tint, shadows and highlights, and the lift, gamma
 *  and gain of each channel. One undo step per slider release, like every other inspector control. */
export function GradeSection({ clip, commit }: { clip: ClipInstance; commit: Commit }) {
  const grade = clip.grade ?? NEUTRAL_GRADE;
  const set = (patch: Partial<Grade>) => {
    return commit({ grade: { ...grade, ...patch } });
  };
  const channel = (key: "lift" | "gamma" | "gain", label: TextKey, min: number, max: number) => {
    return CHANNELS.map(([index, name]) => {
      return (
        <Slider
          key={`${key}-${index}`}
          label={`${t(label)} ${t(name)}`}
          value={grade[key][index]}
          min={min}
          max={max}
          step={0.01}
          format={format}
          onCommit={(v) => {
            const next = [...grade[key]] as [number, number, number];
            next[index] = v;
            return set({ [key]: next });
          }}
        />
      );
    });
  };
  return (
    <Section
      group="effects"
      label={t("colorGrade")}
      defaultOpen={!gradeIsNeutral(grade)}
      onReset={() => {
        void commit({ grade: NEUTRAL_GRADE });
      }}
    >
      <Slider
        label={t("gradeExposure")}
        value={grade.exposure}
        min={-MAX_GRADE_EXPOSURE}
        max={MAX_GRADE_EXPOSURE}
        step={0.05}
        format={format}
        onCommit={(v) => {
          return set({ exposure: v });
        }}
      />
      <Slider
        label={t("gradeTemperature")}
        value={grade.temperature}
        min={-1}
        max={1}
        step={0.01}
        format={format}
        onCommit={(v) => {
          return set({ temperature: v });
        }}
      />
      <Slider
        label={t("gradeTint")}
        value={grade.tint}
        min={-1}
        max={1}
        step={0.01}
        format={format}
        onCommit={(v) => {
          return set({ tint: v });
        }}
      />
      <Slider
        label={t("gradeShadows")}
        value={grade.shadows}
        min={-1}
        max={1}
        step={0.01}
        format={format}
        onCommit={(v) => {
          return set({ shadows: v });
        }}
      />
      <Slider
        label={t("gradeHighlights")}
        value={grade.highlights}
        min={-1}
        max={1}
        step={0.01}
        format={format}
        onCommit={(v) => {
          return set({ highlights: v });
        }}
      />
      {channel("lift", "gradeLift", -0.5, 0.5)}
      {channel("gamma", "gradeGamma", 0.2, 5)}
      {channel("gain", "gradeGain", 0, 4)}
    </Section>
  );
}
