declare namespace NodeJS {
  interface ProcessEnv {
    [key: string]: string | undefined;
    TAURI_DEV_HOST?: string;
  }
  interface Process {
    env: ProcessEnv;
  }
}

declare const process: NodeJS.Process;
