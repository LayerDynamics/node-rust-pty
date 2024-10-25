# node-rust-pty

A high-performance Node.js native module providing comprehensive PTY (pseudo-terminal) functionality with virtual DOM-based rendering and advanced session management, implemented in Rust with N-API bindings.

## Core Features & Implementation Details

### PTY Process Management

#### Process Creation and Lifecycle
```typescript
// Process initialization
const pty = await PtyHandle.new({
  env: process.env,           // Environment variables
  cwd: process.cwd(),        // Working directory
  defaultShell: true         // Use system default shell
});

// Status monitoring
const status = await pty.status();
// Returns: "Running" | "Not Running" | "Unknown"

// Graceful shutdown with timeout
await pty.close({timeout: 5000}); 
```

#### Platform-Specific Implementations
- **Linux**
  - Uses `/dev/pts` for PTY allocation
  - Full `termios` settings support
  - Process group handling
  ```rust
  // Linux PTY creation (internal)
  let master_fd = unsafe { posix_openpt(O_RDWR | O_NOCTTY) };
  unsafe { grantpt(master_fd) };
  unsafe { unlockpt(master_fd) };
  ```

- **macOS**
  - Native PTY through BSD API
  - Terminal attribute management
  - Process group signals
  ```rust
  // macOS PTY handling (internal)
  let mut master: libc::c_int = 0;
  let mut slave: libc::c_int = 0;
  let ret = unsafe { openpty(&mut master, &mut slave, ptr::null_mut(), ptr::null_mut(), ptr::null_mut()) };
  ```

- **Windows**
  - Basic process spawning
  - Environment handling
  - Working directory support

### Virtual DOM Terminal Engine

#### Core Virtual DOM Types
```typescript
interface VNode {
  type: 'text' | 'element';
  content: string | VElement;
}

interface VElement {
  tag: string;
  props: {
    content?: string;
    id?: string;
    class?: string;
    [key: string]: string | undefined;
  };
  styles: {
    color?: 'black' | 'red' | 'green' | 'yellow' | 'blue' | 'magenta' | 'cyan' | 'white';
    background?: string;
    bold?: 'true' | 'false';
    underline?: 'true' | 'false';
    italic?: 'true' | 'false';
    strikethrough?: 'true' | 'false';
    [key: string]: string | undefined;
  };
  children: VNode[];
}
```

#### Diff Engine Implementation
```typescript
// Available patch types
type Patch = 
  | { type: 'Replace'; node: VNode }
  | { type: 'UpdateProps'; props: Record<string, string> }
  | { type: 'AppendChild'; node: VNode }
  | { type: 'RemoveChild'; index: number }
  | { type: 'UpdateText'; text: string }
  | { type: 'None' };

// Usage example
const oldNode = createTextNode("Hello");
const newNode = createTextNode("Hello World");
const patches = diff(oldNode, newNode);
// Results in: [{ type: 'UpdateText', text: 'Hello World' }]
```

#### Renderer Capabilities
```typescript
// Text styling example
const styledOutput = {
  type: 'element',
  content: {
    tag: 'text',
    props: { content: 'Styled Output' },
    styles: {
      color: 'blue',
      background: 'white',
      bold: 'true',
      underline: 'true'
    },
    children: []
  }
};

// Render update
await pty.render(styledOutput);
```

### Session Management System

#### Session Creation and Control
```typescript
// Create multiple sessions
const session1 = await pty.createSession();
const session2 = await pty.createSession();

// Session operations
interface SessionOperations {
  // Send data to specific session
  sendToSession(
    sessionId: number, 
    data: Buffer, 
    options?: {
      flush?: boolean,      // Flush buffer immediately
      encoding?: string,    // Buffer encoding
      timeout?: number      // Operation timeout
    }
  ): Promise<void>;

  // Read from session with options
  readFromSession(
    sessionId: number,
    options?: {
      maxBytes?: number,    // Maximum bytes to read
      timeout?: number,     // Read timeout
      encoding?: string     // Output encoding
    }
  ): Promise<string>;

  // Merge sessions
  mergeSessions(
    sessionIds: number[],
    options?: {
      primary?: number     // Primary session ID
    }
  ): Promise<void>;

  // Split session
  splitSession(
    sessionId: number
  ): Promise<[number, number]>;
}
```

#### Session Buffer Management
```typescript
// Internal buffer management (Rust)
pub struct SessionBuffer {
    input_buffer: Vec<u8>,
    output_buffer: Arc<Mutex<Vec<u8>>>,
    max_buffer_size: usize,
    encoding: String,
}

// Buffer operations
impl SessionBuffer {
    pub fn write(&mut self, data: &[u8]) -> io::Result<usize>;
    pub fn read(&mut self, max_bytes: Option<usize>) -> io::Result<Vec<u8>>;
    pub fn flush(&mut self) -> io::Result<()>;
    pub fn clear(&mut self);
}
```

### Text Processing Engine

#### ASCII/ANSI Parser
```typescript
// Supported ANSI sequences
const ANSISupport = {
  cursor: {
    up: '\x1b[{n}A',
    down: '\x1b[{n}B',
    forward: '\x1b[{n}C',
    backward: '\x1b[{n}D',
    nextLine: '\x1b[{n}E',
    prevLine: '\x1b[{n}F',
    setPosition: '\x1b[{line};{column}H'
  },
  clear: {
    screen: '\x1b[2J',
    line: '\x1b[2K',
    toEnd: '\x1b[0J',
    toStart: '\x1b[1J'
  },
  style: {
    reset: '\x1b[0m',
    bold: '\x1b[1m',
    dim: '\x1b[2m',
    italic: '\x1b[3m',
    underline: '\x1b[4m',
    reverse: '\x1b[7m'
  },
  color: {
    // Foreground colors
    black: '\x1b[30m',
    red: '\x1b[31m',
    green: '\x1b[32m',
    yellow: '\x1b[33m',
    blue: '\x1b[34m',
    magenta: '\x1b[35m',
    cyan: '\x1b[36m',
    white: '\x1b[37m',
    // Background colors
    bgBlack: '\x1b[40m',
    bgRed: '\x1b[41m',
    bgGreen: '\x1b[42m',
    bgYellow: '\x1b[43m',
    bgBlue: '\x1b[44m',
    bgMagenta: '\x1b[45m',
    bgCyan: '\x1b[46m',
    bgWhite: '\x1b[47m'
  }
};
```

#### Input Event Handling
```typescript
// Available input events
interface InputEvent {
  type: 'keypress' | 'keydown' | 'keyup';
  key: string;
  modifiers: {
    ctrl: boolean;
    alt: boolean;
    shift: boolean;
    meta: boolean;
  };
  raw: number[];  // Raw byte sequence
}

// Event handling example
pty.on('input', (event: InputEvent) => {
  if (event.type === 'keypress' && event.modifiers.ctrl && event.key === 'c') {
    // Handle Ctrl+C
  }
});
```

## Installation & Setup

### Prerequisites
```bash
# Linux build dependencies
sudo apt-get install -y \
  build-essential \
  pkg-config \
  libssl-dev

# macOS build dependencies
xcode-select --install

# Windows build dependencies
# Ensure you have Visual Studio Build Tools with Windows SDK
```

### Installation
```bash
# NPM installation
npm install node-rust-pty

# Yarn installation
yarn add node-rust-pty

# Build from source
git clone https://github.com/yourusername/node-rust-pty.git
cd node-rust-pty
npm install
npm run build
```

## Usage Examples

### Basic Terminal Session
```typescript
import { PtyHandle } from 'node-rust-pty';

async function basicTerminal() {
  const pty = await PtyHandle.new();
  const session = await pty.createSession();

  // Execute command
  await pty.sendToSession(session, Buffer.from('ls -la\n'));
  
  // Read with timeout
  const output = await pty.readFromSession(session, {
    timeout: 1000,
    maxBytes: 4096
  });
  
  console.log('Command output:', output);
  await pty.removeSession(session);
  await pty.close();
}
```

### Styled Terminal Output
```typescript
import { PtyHandle, VNode } from 'node-rust-pty';

async function styledOutput() {
  const pty = await PtyHandle.new();
  const session = await pty.createSession();

  // Create styled header
  const header: VNode = {
    type: 'element',
    content: {
      tag: 'text',
      props: {
        content: '=== Terminal Output ===\n',
        id: 'header'
      },
      styles: {
        color: 'blue',
        bold: 'true',
        underline: 'true'
      },
      children: []
    }
  };

  // Create content node
  const content: VNode = {
    type: 'element',
    content: {
      tag: 'text',
      props: {
        content: 'Command output below:\n',
        class: 'output'
      },
      styles: {
        color: 'green'
      },
      children: []
    }
  };

  // Render both nodes
  await pty.render(header);
  await pty.render(content);
  
  // Execute command with styled output
  await pty.sendToSession(session, Buffer.from('echo "Hello World"\n'));
  
  await pty.close();
}
```

### Multiple Session Management
```typescript
import { PtyHandle } from 'node-rust-pty';

async function multiSession() {
  const pty = await PtyHandle.new();
  
  // Create multiple sessions
  const sessions = await Promise.all([
    pty.createSession(),
    pty.createSession(),
    pty.createSession()
  ]);
  
  // Send different commands to each session
  await Promise.all([
    pty.sendToSession(sessions[0], Buffer.from('echo "Session 1"\n')),
    pty.sendToSession(sessions[1], Buffer.from('echo "Session 2"\n')),
    pty.sendToSession(sessions[2], Buffer.from('echo "Session 3"\n'))
  ]);
  
  // Read from all sessions
  const outputs = await Promise.all(
    sessions.map(sid => pty.readFromSession(sid))
  );
  
  // Merge first two sessions
  await pty.mergeSessions([sessions[0], sessions[1]]);
  
  // Split remaining session
  const [newSession1, newSession2] = await pty.splitSession(sessions[2]);
  
  // Cleanup
  await pty.closeAllSessions();
  await pty.close();
}
```

## Configuration

### TypeScript Configuration
```json
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "NodeNext",
    "moduleResolution": "NodeNext",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "declaration": true,
    "declarationMap": true,
    "sourceMap": true
  },
  "include": [
    "index.ts",
    "index.d.ts",
    "npm/prebuilds/*.d.ts"
  ]
}
```

### Build Configuration
```json
{
  "napi": {
    "name": "node-rust-pty",
    "triples": {
      "defaults": true,
      "additional": [
        "aarch64-apple-darwin",
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu"
      ]
    }
  }
}
```

## Development Commands
```bash
# Build commands
npm run build              # Build everything
npm run build:ts          # Build TypeScript only
npm run build:debug       # Debug build
npm run clean            # Clean build artifacts

# Testing
npm test                 # Run all tests
npm test -- --watch     # Watch mode

# Publishing
npm run prepublishOnly  # Prepare for publishing
```

## Current Limitations

1. PTY Process Limitations:
   - Windows support lacks full ConPTY integration
   - Limited process group signal handling on Windows
   - No job control support

2. Terminal Emulation:
   - Subset of VT100/VT220 sequences supported
   - Limited mouse input support
   - Basic color palette (8 colors + backgrounds)

3. Session Management:
   - In-memory session storage only
   - No persistent sessions
   - Limited session sharing capabilities

4. Performance Considerations:
   - Large output buffers may impact memory usage
   - Virtual DOM updates may be CPU intensive
   - No hardware acceleration

## License

MIT License - see [LICENSE](LICENSE) for details.

## Support & Documentation

- Issue Tracker: GitHub Issues
- API Documentation: See `docs/` directory
- Examples: See `examples/` directory
- Technical Documentation: See `docs/technical/`

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for detailed contribution guidelines.

Key areas for contribution:
1. Windows ConPTY integration
2. Additional terminal sequences
3. Performance optimizations
4. Testing improvements

## Credits

Built with:
- [Rust](https://www.rust-lang.org/)
- [N-API](https://nodejs.org/api/n-api.html)
- [Tokio](https://tokio.rs/)
- [TypeScript](https://www.typescriptlang.org/)

## Version History

See [CHANGELOG.md](CHANGELOG.md) for detailed version history.