export type ConformanceVectorGroup = "required" | "optional";

export interface ConformanceVector {
  id: string;
  group: ConformanceVectorGroup;
  run(): void;
}

export type ConformanceResult =
  | { id: string; group: ConformanceVectorGroup; status: "passed" }
  | { id: string; group: ConformanceVectorGroup; status: "failed"; message: string };

export class ConformanceRunner {
  constructor(private readonly vectors: ConformanceVector[]) {}

  runAll(): ConformanceResult[] {
    return this.vectors.map((vector) => {
      try {
        vector.run();
        return { id: vector.id, group: vector.group, status: "passed" };
      } catch (error) {
        return {
          id: vector.id,
          group: vector.group,
          status: "failed",
          message: error instanceof Error ? error.message : String(error)
        };
      }
    });
  }
}
