// Use the pinned compiler as static evidence, not as the business rulebook.
import ts from 'typescript';
import { DiagnosticFailure } from '../diagnostics/internal.mjs';

// Diagnostics from the pinned compiler's noImplicitAny/noImplicitThis checks
// in executable .ts. Missing declarations (TS7016) are a binding issue.
const implicitAny = new Set([
  2683, 7005, 7006, 7008, 7009, 7010, 7011, 7012, 7013, 7014, 7015,
  7017, 7018, 7019, 7020, 7022, 7023, 7024, 7025, 7031, 7032, 7033,
  7034, 7039, 7052, 7053, 7055, 7057,
]);

function at(file, start, label) {
  if (start === undefined || !file) return label;
  const {line, character} = file.getLineAndCharacterOfPosition(start);
  return `${label ?? file.fileName}:${line + 1}:${character + 1}`;
}

export function rejectAny(message, file, start, label, toolCode) {
  throw new DiagnosticFailure('NORM-TS-ANY', 'Panic: not-support; ' + message,
    {location: at(file, start, label), toolCode});
}

export function rejectDirective(message, file, start, label) {
  const location = at(file, start, label ?? file.fileName);
  throw new DiagnosticFailure('NORM-TS-CHECK-DIRECTIVE', message, {location});
}

export function rejectCompilerDiagnostic(error, {syntax = false, label} = {}) {
  const toolCode = `TS${error.code}`;
  let message = ts.flattenDiagnosticMessageText(error.messageText, '\n');
  // TS2584 suggests adding DOM to lib. That would violate AI-01.
  if (error.code === 2584 && error.file && error.start !== undefined && error.length) {
    const name = error.file.text.slice(error.start, error.start + error.length);
    message = `Cannot find name '${name}'. Host libraries are not loaded; preprocess host references before admission.`;
  }
  // Compiler auto-fixes for module resolution can recommend changing
  // tsconfig or copying a physical path. Neither is an admitted remedy.
  if ([2613, 2614, 2792].includes(error.code)) message = message.split(' Did you mean')[0];
  if (!syntax && implicitAny.has(error.code)) {
    rejectAny(message, error.file, error.start, label, toolCode);
  }
  throw new DiagnosticFailure(syntax ? 'NORM-TS-SYNTAX' : 'NORM-TS-TYPECHECK', message,
    {location: at(error.file, error.start, label), toolCode});
}
