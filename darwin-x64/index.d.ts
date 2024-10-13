/* tslint:disable */
/* eslint-disable */

/* auto-generated by NAPI-RS */

/**
 * A test function to verify the creation, usage, and closure of a `PtyHandle` instance.
 *
 * This function performs the following steps:
 *
 * 1. Creates a new `PtyHandle`.
 * 2. Resizes the PTY window.
 * 3. Attempts to read from the PTY with a timeout.
 * 4. Writes a simple command to the PTY.
 * 5. Closes the PTY gracefully.
 *
 * Returns `true` if all operations are successful.
 */
export declare function testPtyHandleCreation(): Promise<boolean>
/** Represents a handle to a PTY process, providing methods to interact with it. */
export declare class PtyHandle {
  static new(): Promise<PtyHandle>
  /** Asynchronously reads data from the PTY. */
  read(): Promise<string>
  /** Asynchronously writes data to the PTY. */
  write(data: string): Promise<void>
  /** Asynchronously resizes the PTY window. */
  resize(cols: number, rows: number): Promise<void>
  /** Asynchronously closes the PTY gracefully. */
  close(): Promise<void>
}