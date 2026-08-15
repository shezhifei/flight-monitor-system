"""
Shadow Mode Operator Feedback Interface.

A React/TypeScript component for human operators to review AI responses,
provide manual answers, and submit feedback during shadow mode deployment.

Usage:
    import { ShadowModeOperatorUI } from './ShadowModeOperatorUI';
    
    <ShadowModeOperatorUI 
        operatorId="operator_001"
        onSubmit={handleHumanAnswer}
    />
"""

import React, { useState, useEffect, useCallback } from 'react';
import { Card, Button, Textarea, Radio, Select, Alert, Spinner, Badge } from '@nextui-org/react';
import { v4 as uuidv4 } from 'uuid';

// Type definitions
interface QueryTask {
    query_id: string;
    user_query: string;
    context: Record<string, any>;
    assigned_at: string;
    priority: 'low' | 'normal' | 'high' | 'urgent';
    source: string;
}

interface ComparisonResult {
    discrepancies: Array<{
        type: string;
        field?: string;
        human_value?: any;
        agent_value?: any;
        severity: 'critical' | 'major' | 'minor' | 'informational';
        description: string;
    }>;
    max_severity: string;
    overall_agreement: number;
}

interface OperatorFeedbackProps {
    operatorId: string;
    onSubmit: (query_id: string, answer: object, confidence: number) => Promise<void>;
    autoFetchInterval?: number; // milliseconds between auto-refresh
}

export const ShadowModeOperatorUI: React.FC<OperatorFeedbackProps> = ({
    operatorId,
    onSubmit,
    autoFetchInterval = 30000, // 30 seconds default
}) => {
    // State management
    const [currentTask, setCurrentTask] = useState<QueryTask | null>(null);
    const [aiResponse, setAiResponse] = useState<object | null>(null);
    const [humanAnswer, setHumanAnswer] = useState<string>('');
    const [confidenceLevel, setConfidenceLevel] = useState<number>(0.5);
    const [isProcessing, setIsProcessing] = useState<boolean>(false);
    const [comparisonResult, setComparisonResult] = useState<ComparisonResult | null>(null);
    const [lastUpdate, setLastUpdate] = useState<Date>(new Date());
    
    // Fetch next task from queue
    const fetchNextTask = useCallback(async () => {
        try {
            // TODO: Replace with actual API call to your human queue
            const response = await fetch('/api/v1/shadow/human-queue/next', {
                method: 'GET',
                headers: {
                    'Content-Type': 'application/json',
                    'Authorization': `Bearer ${localStorage.getItem('operator_token')}`,
                },
            });
            
            if (!response.ok) {
                throw new Error(`HTTP error! status: ${response.status}`);
            }
            
            const data = await response.json();
            setCurrentTask(data.task);
            
            // Also fetch AI response for comparison
            if (data.query_id) {
                const aiResponseResp = await fetch(`/api/v1/shadow/ai-response/${data.query_id}`, {
                    method: 'GET',
                });
                
                if (aiResponseResp.ok) {
                    const aiData = await aiResponseResp.json();
                    setAiResponse(aiData.response);
                    
                    // If both available, get comparison
                    if (comparisonResult !== null) {
                        const compResp = await fetch(
                            `/api/v1/shadow/comparison/${data.query_id}`,
                            { method: 'POST' }
                        );
                        if (compResp.ok) {
                            setComparisonResult(await compResp.json());
                        }
                    }
                }
            }
            
            setLastUpdate(new Date());
            
        } catch (error) {
            console.error('Failed to fetch next task:', error);
        }
    }, [comparisonResult]);
    
    // Auto-refresh queue periodically
    useEffect(() => {
        if (!currentTask) {
            const interval = setInterval(fetchNextTask, autoFetchInterval);
            return () => clearInterval(interval);
        }
    }, [currentTask, autoFetchInterval, fetchNextTask]);
    
    // Submit human answer
    const handleSubmitAnswer = async () => {
        if (!currentTask || !humanAnswer.trim()) {
            alert('Please provide an answer before submitting');
            return;
        }
        
        setIsProcessing(true);
        
        try {
            // Build answer object
            const answerObject = {
                text: humanAnswer,
                confidence: confidenceLevel,
                timestamp: new Date().toISOString(),
                operator_id: operatorId,
            };
            
            // Call submit callback
            await onSubmit(currentTask.query_id, answerObject, confidenceLevel);
            
            // Show success message
            alert('✓ Answer submitted successfully');
            
            // Reset and fetch next task
            setHumanAnswer('');
            setConfidenceLevel(0.5);
            setCurrentTask(null);
            setAiResponse(null);
            setComparisonResult(null);
            
            await fetchNextTask();
            
        } catch (error) {
            console.error('Failed to submit answer:', error);
            alert('✗ Failed to submit answer. Please try again.');
        } finally {
            setIsProcessing(false);
        }
    };
    
    // Skip task (no action needed)
    const handleSkipTask = () => {
        if (currentTask && window.confirm('Skip this task? No answer will be recorded.')) {
            // Mark as skipped via API
            fetch(`/api/v1/shadow/human-queue/skip/${currentTask.query_id}`, {
                method: 'POST',
            }).catch(console.error);
            
            // Reset state and fetch next
            setCurrentTask(null);
            setAiResponse(null);
            setComparisonResult(null);
            setHumanAnswer('');
            setConfidenceLevel(0.5);
        }
    };
    
    // Format date display
    const formatDate = (dateString: string) => {
        const date = new Date(dateString);
        return date.toLocaleTimeString('zh-CN', {
            hour: '2-digit',
            minute: '2-digit',
            second: '2-digit',
        });
    };
    
    // Get priority badge color
    const getPriorityColor = (priority: string) => {
        switch (priority) {
            case 'urgent': return 'danger';
            case 'high': return 'warning';
            case 'normal': return 'primary';
            case 'low': return 'default';
            default: return 'default';
        }
    };
    
    // Calculate time since assignment
    const getTimeSinceAssigned = (assignedAt: string) => {
        const assigned = new Date(assignedAt);
        const now = new Date();
        const minutes = Math.floor((now.getTime() - assigned.getTime()) / 60000);
        
        if (minutes < 1) return '< 1 min';
        if (minutes < 60) return `${minutes} min`;
        
        const hours = Math.floor(minutes / 60);
        return `${hours}h ${minutes % 60}m`;
    };
    
    // Render AI Response section
    const renderAIResponse = () => {
        if (!aiResponse) {
            return (
                <Card className="p-4 mb-4">
                    <Text size="sm" color="default">Loading AI response...</Text>
                </Card>
            );
        }
        
        return (
            <Card className="p-4 mb-4 border-warning border-2">
                <div className="flex justify-between items-center mb-3">
                    <h3 className="text-lg font-bold text-warning">AI Response</h3>
                    <Badge color="warning" variant="flat">Agent Answer</Badge>
                </div>
                
                {typeof aiResponse === 'object' ? (
                    <pre className="bg-gray-50 p-3 rounded-lg text-sm overflow-auto max-h-96">
                        {JSON.stringify(aiResponse, null, 2)}
                    </pre>
                ) : (
                    <Text>{String(aiResponse)}</Text>
                )}
                
                {/* Display comparison result if available */}
                {comparisonResult && (
                    <div className="mt-3 p-3 bg-yellow-50 rounded-lg">
                        <Text size="sm" className="font-semibold">Comparison Status:</Text>
                        <Text size="xs">
                            Discrepancies: {comparisonResult.discrepancy_count} | 
                            Agreement: {(comparisonResult.overall_agreement * 100).toFixed(1)}% |
                            Severity: {comparisonResult.max_severity.toUpperCase()}
                        </Text>
                        
                        {comparisonResult.discrepancies.length > 0 && (
                            <div className="mt-2 space-y-1">
                                {comparisonResult.discrepancies.slice(0, 3).map((disc, idx) => (
                                    <Alert
                                        key={idx}
                                        title={disc.type}
                                        description={disc.description}
                                        color={disc.severity as any}
                                        radius="full"
                                        size="sm"
                                    />
                                ))}
                            </div>
                        )}
                    </div>
                )}
            </Card>
        );
    };
    
    // Render Human Input section
    const renderHumanInput = () => {
        return (
            <Card className="p-4 mt-4">
                <div className="mb-4">
                    <h3 className="text-lg font-bold mb-2">Your Answer</h3>
                    <Textarea
                        label="Manual Analysis & Answer"
                        placeholder="Enter your expert analysis here..."
                        value={humanAnswer}
                        onChange={(e) => setHumanAnswer(e.target.value)}
                        className="w-full"
                        minRows={6}
                        required
                    />
                </div>
                
                <div className="mb-4">
                    <h3 className="text-md font-semibold mb-2">Your Confidence Level</h3>
                    <Radio.Group
                        value={confidenceLevel}
                        onChange={(val) => setConfidenceLevel(Number(val))}
                        orientation="horizontal"
                    >
                        <Radio value={0.3} label="Low (30%)">Low</Radio>
                        <Radio value={0.5} label="Medium (50%)">Medium</Radio>
                        <Radio value={0.7} label="High (70%)">High</Radio>
                        <Radio value={0.9} label="Very High (90%)">Very High</Radio>
                        <Radio value={1.0} label="Certain (100%)">Certain</Radio>
                    </Radio.Group>
                </div>
                
                <div className="flex gap-2">
                    <Button
                        color="success"
                        size="lg"
                        onPress={handleSubmitAnswer}
                        isLoading={isProcessing}
                        isDisabled={!humanAnswer.trim()}
                    >
                        ✓ Submit Answer
                    </Button>
                    
                    <Button
                        color="default"
                        size="lg"
                        variant="bordered"
                        onPress={handleSkipTask}
                    >
                        ⏭ Skip
                    </Button>
                </div>
            </Card>
        );
    };
    
    // Main render
    return (
        <div className="container mx-auto p-6 max-w-4xl">
            {/* Header */}
            <div className="flex justify-between items-center mb-6">
                <div>
                    <h1 className="text-2xl font-bold">Shadow Mode Operator Interface</h1>
                    <Text size="sm" color="secondary">
                        Operator ID: {operatorId} • Last updated: {formatDate(lastUpdate.toISOString())}
                    </Text>
                </div>
                
                <div className="text-right">
                    {currentTask && (
                        <>
                            <Badge color={getPriorityColor(currentTask.priority)} variant="rounded">
                                {currentTask.priority.toUpperCase()}
                            </Badge>
                            <Text size="sm" className="ml-4">
                                Time: {getTimeSinceAssigned(currentTask.assigned_at)}
                            </Text>
                        </>
                    )}
                </div>
            </div>
            
            {/* Current Task Display */}
            {currentTask ? (
                <div className="space-y-4">
                    {/* Task Card */}
                    <Card className="p-4 mb-4">
                        <h3 className="text-md font-semibold mb-2">Original Query</h3>
                        <Text size="lg">{currentTask.user_query}</Text>
                        
                        {currentTask.context && Object.keys(currentTask.context).length > 0 && (
                            <details className="mt-3">
                                <summary className="text-sm cursor-pointer text-primary">Show Context</summary>
                                <pre className="bg-gray-50 p-3 rounded-lg text-xs mt-2 overflow-auto">
                                    {JSON.stringify(currentTask.context, null, 2)}
                                </pre>
                            </details>
                        )}
                    </Card>
                    
                    {/* AI Response */}
                    {renderAIResponse()}
                    
                    {/* Human Input */}
                    {renderHumanInput()}
                </div>
            ) : (
                /* Empty State */
                <Card className="p-8 text-center">
                    <Spinner size="lg" />
                    <Text size="lg" className="mt-4">Checking for tasks...</Text>
                    <Text size="sm" color="secondary">
                        Next available task will appear automatically
                    </Text>
                </Card>
            )}
            
            {/* Footer info */}
            <div className="mt-8 text-center text-xs text-secondary">
                <Text>
                    Shadow Mode Active • All responses are logged for validation • 
                    Do not execute write operations
                </Text>
            </div>
        </div>
    );
};

export default ShadowModeOperatorUI;
