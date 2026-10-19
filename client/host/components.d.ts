// What the host owes this layer beside bmo-ui's own: the pieces of the console's own title bar the
// Files screen stands in. They are declared here by the props the screen hands them, so the typecheck
// holds the screen to what it may ask of the host; the host's own components answer them at build.
import type { DefineComponent } from "vue";

type TitleBarPiece = DefineComponent<Record<string, never>>;

declare module "vue" {
  interface GlobalComponents {
    DeployMark: TitleBarPiece;
    TodoOpenButton: TitleBarPiece;
    UsageMeters: TitleBarPiece;
  }
}

export {};
