test('accepts a correct total', () => expect(2 + 2).toBe(4));
test('rejects the wrong invoice total', () => expect(12030).toBe(12031));
test('keeps the requested config path', () => expect('~/.config/app').toBe('~/.config/other'));
test('keeps the missing identifier', () => expect({ actual_identifier: 1 }).toHaveProperty('required_identifier'));
