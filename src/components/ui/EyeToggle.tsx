interface EyeToggleProps {
  shown: boolean;
  onToggle: () => void;
  className?: string;
}

/** Token 等敏感字段的「明文/掩码」切换按钮（清晰的 SVG 线性眼睛图标）。 */
function EyeToggle({ shown, onToggle, className = "input-toggle" }: EyeToggleProps) {
  return (
    <button
      type="button"
      className={className}
      onClick={onToggle}
      title={shown ? "隐藏" : "显示明文"}
      aria-label={shown ? "隐藏" : "显示明文"}
    >
      {shown ? (
        // 当前明文 → 点击隐藏（eye-off）
        <svg
          width="16"
          height="16"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden="true"
        >
          <path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-10-7-10-7a18.45 18.45 0 0 1 5.06-5.94" />
          <path d="M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 10 7 10 7a18.5 18.5 0 0 1-2.16 3.19" />
          <path d="M14.12 14.12a3 3 0 1 1-4.24-4.24" />
          <line x1="1" y1="1" x2="23" y2="23" />
        </svg>
      ) : (
        // 当前掩码 → 点击显示（eye）
        <svg
          width="16"
          height="16"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden="true"
        >
          <path d="M2 12s3-7 10-7 10 7 10 7-3 7-10 7-10-7-10-7z" />
          <circle cx="12" cy="12" r="3" />
        </svg>
      )}
    </button>
  );
}

export default EyeToggle;
