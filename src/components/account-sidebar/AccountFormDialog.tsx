import { useState, type FormEvent } from "react";

import EyeToggle from "../ui/EyeToggle";
import { errorMessage } from "../../lib/api";
import type { Account, AccountUpdate, NewAccount, Tool } from "../../lib/types";
import { TOOL_LABELS } from "../../lib/types";

interface AccountFormDialogProps {
  mode: "create" | "edit";
  initial: Account | null;
  onCancel: () => void;
  onCreate: (input: NewAccount) => Promise<void>;
  onUpdate: (id: string, patch: AccountUpdate) => Promise<void>;
}

function AccountFormDialog({
  mode,
  initial,
  onCancel,
  onCreate,
  onUpdate,
}: AccountFormDialogProps) {
  const [name, setName] = useState(initial?.name ?? "");
  const [tool, setTool] = useState<Tool>(initial?.tool ?? "claude");
  const [baseUrl, setBaseUrl] = useState(initial?.baseUrl ?? "");
  const [model, setModel] = useState(initial?.model ?? "");
  const [token, setToken] = useState(initial?.token ?? "");
  const [showToken, setShowToken] = useState(true);
  const [tags, setTags] = useState((initial?.tags ?? []).join(", "));
  const [extraArgs, setExtraArgs] = useState(
    (initial?.extraArgs ?? []).join(" "),
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const parseTags = (): string[] =>
    tags
      .split(",")
      .map((t) => t.trim())
      .filter(Boolean);

  const parseExtraArgs = (): string[] =>
    extraArgs
      .split(/\s+/)
      .map((a) => a.trim())
      .filter(Boolean);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      if (mode === "create") {
        const list = parseTags();
        const input: NewAccount = {
          name: name.trim(),
          tool,
          baseUrl: baseUrl.trim(),
          model: model.trim() || undefined,
          token,
          tags: list.length ? list : undefined,
          extraArgs: parseExtraArgs().length ? parseExtraArgs() : undefined,
        };
        await onCreate(input);
      } else if (initial) {
        const list = parseTags();
        const patch: AccountUpdate = {
          name: name.trim(),
          baseUrl: baseUrl.trim(),
          model: model.trim() ? model.trim() : null,
          tags: list.length ? list : null,
          extraArgs: parseExtraArgs().length ? parseExtraArgs() : null,
        };
        patch.token = token;
        await onUpdate(initial.id, patch);
      }
      // 成功后由父组件关闭弹窗
    } catch (err: unknown) {
      setError(errorMessage(err));
      setBusy(false);
    }
  };

  return (
    <div className="dialog-backdrop" onClick={onCancel}>
      <form
        className="dialog"
        onClick={(e) => e.stopPropagation()}
        onSubmit={submit}
      >
        <div className="dialog-head">
          <h2 className="dialog-title">
            {mode === "create" ? "新建账号" : "编辑账号"}
          </h2>
        </div>

        <div className="dialog-body">
          <label className="form-row">
            <span className="form-label">名称</span>
            <input
              className="form-input"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="给账号起个名字"
              autoFocus
              required
            />
          </label>

          <div className="form-row">
            <span className="form-label">工具</span>
            {mode === "create" ? (
              <div className="seg" role="group" aria-label="选择工具">
                {(["claude", "codex"] as Tool[]).map((t) => (
                  <button
                    type="button"
                    key={t}
                    className={"seg-option" + (tool === t ? " is-active" : "")}
                    onClick={() => setTool(t)}
                  >
                    {TOOL_LABELS[t]}
                  </button>
                ))}
              </div>
            ) : (
              <input className="form-input" value={TOOL_LABELS[tool]} disabled />
            )}
          </div>

          <label className="form-row">
            <span className="form-label">BASE_URL</span>
            <input
              className="form-input"
              value={baseUrl}
              onChange={(e) => setBaseUrl(e.target.value)}
              placeholder="https://relay.example.com"
              required
            />
          </label>

          <label className="form-row">
            <span className="form-label">
              默认模型 <i className="form-optional">可选</i>
            </span>
            <input
              className="form-input"
              value={model}
              onChange={(e) => setModel(e.target.value)}
              placeholder={tool === "claude" ? "如 claude-opus-4" : "如 gpt-5-codex"}
            />
          </label>

          <label className="form-row">
            <span className="form-label">Token</span>
            <div className="input-with-toggle">
              <input
                className="form-input"
                type={showToken ? "text" : "password"}
                value={token}
                onChange={(e) => setToken(e.target.value)}
                placeholder="中转 Bearer Token"
                required
              />
              <EyeToggle shown={showToken} onToggle={() => setShowToken((s) => !s)} />
            </div>
          </label>

          <label className="form-row">
            <span className="form-label">
              标签 <i className="form-optional">可选，逗号分隔</i>
            </span>
            <input
              className="form-input"
              value={tags}
              onChange={(e) => setTags(e.target.value)}
              placeholder="如 高额度, 备用"
            />
          </label>

          <label className="form-row">
            <span className="form-label">
              额外启动参数 <i className="form-optional">可选，空格分隔</i>
            </span>
            <input
              className="form-input"
              value={extraArgs}
              onChange={(e) => setExtraArgs(e.target.value)}
              placeholder={
                tool === "claude"
                  ? "如 --dangerously-skip-permissions"
                  : "如 --full-auto"
              }
            />
          </label>

          {error && <p className="form-error">{error}</p>}
        </div>

        <div className="dialog-foot">
          <button
            type="button"
            className="btn btn-ghost"
            onClick={onCancel}
            disabled={busy}
          >
            取消
          </button>
          <button type="submit" className="btn btn-primary" disabled={busy}>
            {busy ? "保存中…" : "保存"}
          </button>
        </div>
      </form>
    </div>
  );
}

export default AccountFormDialog;
