import { readFile } from 'node:fs';
export interface Config { size: number; apply(value: string): number; }
export class Worker {
  async run(input: string): Promise<number> { return Promise.resolve(input.length); }
}
// Convertit une valeur.
export const convert = (value: number): string => value.toString();
export function load<T>(value: T): T { return value; }
export const LIMIT: number = 999;
const text = 'function textualFake() {}';
