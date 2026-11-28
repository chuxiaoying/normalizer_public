// Private evidence handed to the public CLI renderer, not a second public protocol.
export class DiagnosticFailure extends Error {
  constructor(code, message, {location, target, toolCode} = {}) {
    super(message);
    this.code = code;
    this.location = location;
    this.target = target;
    this.toolCode = toolCode;
  }
}

const inputIo = new Set(['ENOENT', 'EACCES', 'ENOTDIR', 'EISDIR', 'ENOSPC', 'EPERM']);

export function reportFailure(error) {
  let diagnostic = error;
  if (!(error instanceof DiagnosticFailure) && inputIo.has(error?.code)) {
    const temporary = ['mkdtemp', 'unlink', 'rmdir'].includes(error.syscall);
    diagnostic = new DiagnosticFailure(temporary ? 'NORM-TEMP-IO' : 'NORM-INPUT-IO', error.message,
      {location: error.path});
  }
  if (diagnostic instanceof DiagnosticFailure) {
    const {code, message, location, target, toolCode: tool_code} = diagnostic;
    process.stderr.write('@normalizer-diagnostic ' + JSON.stringify({code, message, location, target, tool_code}) + '\n');
  } else {
    // Transitional bridge for stage diagnostics that predate typed evidence.
    process.stderr.write(String(error?.message ?? error) + '\n');
  }
}
