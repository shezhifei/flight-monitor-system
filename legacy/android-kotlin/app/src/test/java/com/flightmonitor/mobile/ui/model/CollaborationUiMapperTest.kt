package com.flightmonitor.mobile.ui.model

import com.flightmonitor.mobile.api.model.DispatchChatGroupSummary
import com.flightmonitor.mobile.api.model.NotificationReceipt
import com.flightmonitor.mobile.api.model.NotificationReceiptGroup
import com.flightmonitor.mobile.api.model.NotificationReceiptSummary
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class CollaborationUiMapperTest {
    @Test
    fun `workflow maps to 流程`() {
        val result = CollaborationUiMapper.mapOrigin("workflow", null)
        assertEquals(OriginKind.WORKFLOW, result.kind)
        assertEquals("流程", result.label)
    }

    @Test
    fun `manual maps to 人工`() {
        val result = CollaborationUiMapper.mapOrigin("manual", null)
        assertEquals(OriginKind.MANUAL, result.kind)
        assertEquals("人工", result.label)
    }

    @Test
    fun `rejected without note fails validation`() {
        val error = CollaborationUiMapper.validateAcknowledgement("rejected", "   ")
        assertEquals("拒绝通知必须填写理由", error)
    }

    @Test
    fun `acknowledged without note is allowed`() {
        val error = CollaborationUiMapper.validateAcknowledgement("acknowledged", null)
        assertNull(error)
    }

    @Test
    fun `processed notification becomes read only`() {
        val state = CollaborationUiMapper.mapNotificationActionState("acknowledged", receiptRequired = true)
        assertFalse(state.canSubmit)
        assertEquals("已确认", state.finalStatusLabel)
    }

    @Test
    fun `receipt summary parses counts and group ids`() {
        val summary = CollaborationUiMapper.mapReceiptSummary(
            mapOf(
                "total_count" to 5,
                "pending_count" to 2,
                "acknowledged_count" to 2,
                "rejected_count" to 1,
                "latest_updated_at" to "2026-03-08T10:00:00Z",
                "receipt_group_ids" to listOf("rg-1", "rg-2"),
            ),
        )
        assertEquals(5, summary.totalCount)
        assertEquals(2, summary.pendingCount)
        assertEquals(2, summary.acknowledgedCount)
        assertEquals(1, summary.rejectedCount)
        assertEquals(listOf("rg-1", "rg-2"), summary.receiptGroupIds)
    }

    @Test
    fun `receipt group mapper uses backend summary directly`() {
        val group = NotificationReceiptGroup(
            receipt_group_id = "rg-1",
            title = "Dispatch order assigned",
            origin_type = "workflow",
            origin_label = "流程",
            receipt_required = true,
            summary = NotificationReceiptSummary(
                total_count = 3,
                pending_count = 1,
                acknowledged_count = 1,
                rejected_count = 1,
                latest_updated_at = "2026-03-08T11:00:00Z",
            ),
            items = listOf(
                NotificationReceipt(
                    notification_id = "n-1",
                    user_id = "u-1",
                    title = "Dispatch order assigned",
                    origin_type = "workflow",
                    origin_label = "流程",
                    receipt_group_id = "rg-1",
                    delivery_status = "sent",
                    read_status = "read",
                    ack_status = "rejected",
                    ack_note = "现场冲突",
                    updated_at = "2026-03-08T11:00:00Z",
                ),
            ),
        )
        val result = CollaborationUiMapper.mapReceiptGroup(group)
        assertEquals(3, result.summary.totalCount)
        assertEquals(1, result.summary.rejectedCount)
        assertEquals("现场冲突", result.items.first().ackNote)
    }

    @Test
    fun `read only member cannot send`() {
        val state = CollaborationUiMapper.mapChatComposerState(
            DispatchChatGroupSummary(
                group_id = "g-1",
                channel_type = "dispatch",
                flight_id = "CA123",
                group_name = "G1",
                read_only = true,
            ),
        )
        assertFalse(state.canSend)
        assertTrue(state.readOnlyHint!!.contains("只读成员"))
    }

    @Test
    fun `active member can still send`() {
        val state = CollaborationUiMapper.mapChatComposerState(
            DispatchChatGroupSummary(
                group_id = "g-1",
                channel_type = "dispatch",
                flight_id = "CA123",
                group_name = "G1",
                read_only = false,
            ),
        )
        assertTrue(state.canSend)
        assertNull(state.readOnlyHint)
    }
}
