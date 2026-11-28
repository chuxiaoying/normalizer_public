interface Contact {
    readonly label: string | null;
    readonly useStored: boolean;
}

export function label(contact: Contact): string | null {
    return contact.useStored ? contact.label : null;
}
