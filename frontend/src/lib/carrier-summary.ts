import type { CarrierEventRow } from './types/carrier';
import { formatNumber, formatCredits } from './format';

/**
 * One-line description of a history row, for the log table's Detail column.
 *
 * Pure — no reactivity, no DOM — so it can be unit-tested or reused elsewhere.
 * Narrows on `json_data.event`, so each branch only sees its own fields.
 */
export function eventSummary(row: CarrierEventRow): string {
	const e = row.json_data;
	switch (e.event) {
		case 'CarrierDepositFuel':
			return `fuel +${formatNumber(e.Amount)} → ${formatNumber(e.Total)}`;
		case 'CarrierJumpRequest':
			return `jump → ${e.SystemName}`;
		case 'CarrierJump':
			return `arrived ${e.StarSystem}`;
		case 'CarrierLocation':
			return `at ${e.StarSystem}`;
		case 'CarrierNameChange':
			return `renamed “${e.Name}”`;
		case 'CarrierFinance':
			return `balance ${formatCredits(e.CarrierBalance)}`;
		case 'CarrierBankTransfer':
			return e.Deposit
				? `deposit ${formatCredits(e.Deposit)}`
				: `withdraw ${formatCredits(e.Withdraw)}`;
		case 'CarrierCrewServices':
			return `${e.Operation} ${e.CrewRole}`;
		case 'CarrierTradeOrder':
			return `${e.Commodity} @ ${formatCredits(e.Price)} CR`;
		case 'CarrierBuy':
			return `bought for ${formatCredits(e.Price)} at ${e.Location}`;
		case 'CarrierDockingPermission':
			return `${e.DockingAccess}${e.AllowNotorious ? ' (notorious ok)' : ''}`;
		case 'CarrierStats':
			return `Openned the Carrier Managment Menu`;
		default:
			return e.event;
	}
}
