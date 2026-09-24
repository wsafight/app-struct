import { RefreshCw } from "lucide-react";

export function AsyncState({
  state,
  message,
  onRetry,
}: {
  state: "loading" | "empty" | "error";
  message: string;
  onRetry?: () => void;
}) {
  return (
    <div
      className={`async-state async-state-${state}`}
      role={state === "error" ? "alert" : "status"}
      aria-live={state === "error" ? "assertive" : "polite"}
    >
      <span>{message}</span>
      {state === "error" && onRetry && (
        <button type="button" className="secondary-button" onClick={onRetry}>
          <RefreshCw size={16} /> Retry
        </button>
      )}
    </div>
  );
}
