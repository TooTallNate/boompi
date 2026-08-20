import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@boompi/ui/components/alert-dialog";
import { Button } from "@boompi/ui/components/button";
import type { ComponentProps, ReactNode } from "react";

/** A Button whose action deserves a second thought: opens an
 *  AlertDialog instead of firing immediately. Replaces
 *  `window.confirm` (blocking, unstyled, and suppressed outright by
 *  some webviews/iframes). The confirm action inherits the trigger's
 *  variant, so a destructive button gets a destructive confirm. */
export function ConfirmButton({
  title,
  description,
  confirmLabel,
  confirmVariant,
  onConfirm,
  children,
  ...button
}: ComponentProps<typeof Button> & {
  title: string;
  description?: ReactNode;
  confirmLabel: string;
  /** Confirm action styling when the trigger's variant isn't right
   *  (e.g. a ghost trigger whose action is destructive). */
  confirmVariant?: ComponentProps<typeof Button>["variant"];
  onConfirm: () => void;
}) {
  return (
    <AlertDialog>
      <AlertDialogTrigger asChild>
        <Button {...button}>{children}</Button>
      </AlertDialogTrigger>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{title}</AlertDialogTitle>
          {description != null && (
            <AlertDialogDescription>{description}</AlertDialogDescription>
          )}
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction
            variant={confirmVariant ?? button.variant}
            onClick={onConfirm}
          >
            {confirmLabel}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
