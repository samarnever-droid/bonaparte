import type { Project } from "./model";
const dbName = "bonaparte-recovery-v2";
async function db(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(dbName, 1);
    request.onupgradeneeded = () => request.result.createObjectStore("recovery");
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}
export async function saveRecovery(project: Project | string, version = 3): Promise<void> {
  const database = await db();
  return new Promise((resolve, reject) => {
    const transaction = database.transaction("recovery", "readwrite");
    transaction
      .objectStore("recovery")
      .put(
        typeof project === "string"
          ? project
          : JSON.stringify({ format: "bonaparte", version, project }),
        "latest",
      );
    transaction.oncomplete = () => {
      database.close();
      resolve();
    };
    transaction.onerror = () => {
      database.close();
      reject(transaction.error);
    };
  });
}
export async function readRecovery(): Promise<string | null> {
  const database = await db();
  return new Promise((resolve, reject) => {
    const request = database.transaction("recovery").objectStore("recovery").get("latest");
    request.onsuccess = () => {
      database.close();
      resolve(request.result ?? null);
    };
    request.onerror = () => {
      database.close();
      reject(request.error);
    };
  });
}
