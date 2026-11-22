import { useRef, useState, type ChangeEvent, type CompositionEvent } from "react";
import { hasInvalidLibraryNameCharacters, libraryNameError } from "@/lib/libraryPath";
/** Reject unsupported edits without rewriting names or interrupting IME composition. */
export function useLibraryNameInput(value: string, onChange: (value: string) => void) {
  const [rejected, setRejected] = useState(false);
  const [composing, setComposing] = useState(false);
  const composingRef = useRef(false);
  const beforeComposition = useRef(value);
  const validation = libraryNameError(value);
  const accept = (next: string) => {
    if (hasInvalidLibraryNameCharacters(next)) {
      setRejected(true);
      return;
    }
    setRejected(false);
    onChange(next);
  };
  return {
    error: composing ? null : rejected ? "characters" : value ? validation : null,
    valid: !composing && !rejected && validation === null,
    canSubmit: () => !composingRef.current && !rejected && libraryNameError(value) === null,
    reset: () => {
      composingRef.current = false;
      setComposing(false);
      setRejected(false);
    },
    inputProps: {
      onChange: (event: ChangeEvent<HTMLInputElement>) => {
        const next = event.currentTarget.value;
        if (composingRef.current) onChange(next);
        else accept(next);
      },
      onCompositionStart: () => {
        beforeComposition.current = value;
        composingRef.current = true;
        setComposing(true);
        setRejected(false);
      },
      onCompositionEnd: (event: CompositionEvent<HTMLInputElement>) => {
        composingRef.current = false;
        setComposing(false);
        const next = event.currentTarget.value;
        if (hasInvalidLibraryNameCharacters(next)) {
          onChange(beforeComposition.current);
          setRejected(true);
        } else accept(next);
      },
    },
  };
}
