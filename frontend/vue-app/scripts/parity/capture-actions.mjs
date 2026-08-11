const SAFE_ID_PATTERN = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;
const SUPPORTED_ACTIONS = new Set([
  'check',
  'click',
  'fill',
  'press',
  'select-option',
  'uncheck',
  'wait-for',
]);
const SUPPORTED_WAIT_STATES = new Set(['attached', 'detached', 'hidden', 'visible']);

export class CaptureActionValidationError extends Error {
  constructor(message) {
    super(message);
    this.name = 'CaptureActionValidationError';
  }
}

function assertRecord(value, context) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new CaptureActionValidationError(`${context} must be an object.`);
  }
  return value;
}

function assertNonEmptyString(value, context) {
  if (typeof value !== 'string' || value.trim() === '') {
    throw new CaptureActionValidationError(`${context} must be a non-empty string.`);
  }
  return value;
}

function assertSafeId(value, context) {
  const id = assertNonEmptyString(value, context);
  if (!SAFE_ID_PATTERN.test(id)) {
    throw new CaptureActionValidationError(
      `${context} must use lowercase kebab-case so artifact names remain portable.`,
    );
  }
  return id;
}

function normalizeRegion(value, context) {
  const region = assertRecord(value, context);
  return {
    id: assertSafeId(region.id, `${context}.id`),
    selector: assertNonEmptyString(region.selector, `${context}.selector`),
  };
}

function normalizeAction(value, context) {
  const action = assertRecord(value, context);
  const type = assertNonEmptyString(action.type, `${context}.type`);
  if (!SUPPORTED_ACTIONS.has(type)) {
    throw new CaptureActionValidationError(
      `${context}.type ${JSON.stringify(type)} is unsupported; arbitrary page scripts are not allowed.`,
    );
  }

  const normalized = {
    type,
    selector: assertNonEmptyString(action.selector, `${context}.selector`),
  };
  if (type === 'fill') {
    normalized.value = assertNonEmptyString(action.value, `${context}.value`);
  }
  if (type === 'press') {
    normalized.key = assertNonEmptyString(action.key, `${context}.key`);
  }
  if (type === 'select-option') {
    const values = Array.isArray(action.values) ? action.values : [action.value];
    if (values.length === 0 || values.some((entry) => typeof entry !== 'string')) {
      throw new CaptureActionValidationError(
        `${context}.value or ${context}.values must contain select option strings.`,
      );
    }
    normalized.values = values;
  }
  if (type === 'wait-for') {
    const state = action.state ?? 'visible';
    if (!SUPPORTED_WAIT_STATES.has(state)) {
      throw new CaptureActionValidationError(`${context}.state ${JSON.stringify(state)} is unsupported.`);
    }
    normalized.state = state;
  }
  return normalized;
}

function normalizeUniqueRegions(values, context) {
  if (values === undefined) return [];
  if (!Array.isArray(values)) {
    throw new CaptureActionValidationError(`${context} must be an array.`);
  }
  const regions = values.map((value, index) => normalizeRegion(value, `${context}[${index}]`));
  const duplicate = regions.find((region, index) => (
    regions.findIndex((candidate) => candidate.id === region.id) !== index
  ));
  if (duplicate) {
    throw new CaptureActionValidationError(`${context} contains duplicate region id ${duplicate.id}.`);
  }
  return regions;
}

export function normalizeCaptureDefinition(value, context = 'capture') {
  const capture = value === undefined ? {} : assertRecord(value, context);
  const expectedPanels = capture.expected_panels ?? [];
  if (!Array.isArray(expectedPanels)
    || expectedPanels.some((selector) => typeof selector !== 'string' || selector.trim() === '')) {
    throw new CaptureActionValidationError(`${context}.expected_panels must contain selectors.`);
  }

  const interactionValues = capture.interactions ?? [];
  if (!Array.isArray(interactionValues)) {
    throw new CaptureActionValidationError(`${context}.interactions must be an array.`);
  }
  const interactions = interactionValues.map((value, index) => {
    const interactionContext = `${context}.interactions[${index}]`;
    const interaction = assertRecord(value, interactionContext);
    const actions = interaction.actions;
    if (!Array.isArray(actions) || actions.length === 0) {
      throw new CaptureActionValidationError(`${interactionContext}.actions must not be empty.`);
    }
    const interactionPanels = interaction.expected_panels ?? [];
    if (!Array.isArray(interactionPanels)
      || interactionPanels.some((selector) => typeof selector !== 'string' || selector.trim() === '')) {
      throw new CaptureActionValidationError(
        `${interactionContext}.expected_panels must contain selectors.`,
      );
    }
    return {
      id: assertSafeId(interaction.id, `${interactionContext}.id`),
      actions: actions.map((action, actionIndex) => (
        normalizeAction(action, `${interactionContext}.actions[${actionIndex}]`)
      )),
      expectedPanels: interactionPanels,
      regions: normalizeUniqueRegions(interaction.regions, `${interactionContext}.regions`),
      captureFullPage: interaction.full_page === true,
    };
  });
  const duplicateInteraction = interactions.find((interaction, index) => (
    interactions.findIndex((candidate) => candidate.id === interaction.id) !== index
  ));
  if (duplicateInteraction) {
    throw new CaptureActionValidationError(
      `${context}.interactions contains duplicate id ${duplicateInteraction.id}.`,
    );
  }

  const blockedValues = capture.blocked_interactions ?? [];
  if (!Array.isArray(blockedValues)) {
    throw new CaptureActionValidationError(`${context}.blocked_interactions must be an array.`);
  }
  const blockedInteractions = blockedValues.map((value, index) => {
    const blockedContext = `${context}.blocked_interactions[${index}]`;
    const blocked = assertRecord(value, blockedContext);
    return {
      id: assertSafeId(blocked.id, `${blockedContext}.id`),
      reason: assertNonEmptyString(blocked.reason, `${blockedContext}.reason`),
      source: assertNonEmptyString(blocked.source, `${blockedContext}.source`),
    };
  });
  const allInteractionIds = [...interactions, ...blockedInteractions].map(({ id }) => id);
  const duplicateStateId = allInteractionIds.find((id, index) => allInteractionIds.indexOf(id) !== index);
  if (duplicateStateId) {
    throw new CaptureActionValidationError(
      `${context} contains duplicate executable/blocked interaction id ${duplicateStateId}.`,
    );
  }

  return {
    theme: capture.theme ?? 'light',
    expectedPanels,
    regions: normalizeUniqueRegions(capture.regions, `${context}.regions`),
    captureFullPage: capture.full_page !== false,
    interactions,
    blockedInteractions,
  };
}

export async function runCaptureActions(page, actions) {
  for (const action of actions) {
    const locator = page.locator(action.selector).first();
    switch (action.type) {
      case 'click':
        await locator.waitFor({ state: 'visible' });
        await locator.click();
        break;
      case 'fill':
        await locator.waitFor({ state: 'visible' });
        await locator.fill(action.value);
        break;
      case 'press':
        await locator.waitFor({ state: 'visible' });
        await locator.press(action.key);
        break;
      case 'select-option':
        await locator.waitFor({ state: 'visible' });
        await locator.selectOption(action.values);
        break;
      case 'check':
        await locator.waitFor({ state: 'visible' });
        await locator.check();
        break;
      case 'uncheck':
        await locator.waitFor({ state: 'visible' });
        await locator.uncheck();
        break;
      case 'wait-for':
        await locator.waitFor({ state: action.state });
        break;
      default:
        throw new CaptureActionValidationError(`Unsupported normalized action ${action.type}.`);
    }
  }
}
