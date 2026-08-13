use fms_domain::ports::ai_object_policy_repository::AiObjectPolicySubject;

pub(crate) fn object_policy_subject(
    actor_id: &str,
    actor_permissions: &[String],
    actor_department_id: Option<&str>,
) -> AiObjectPolicySubject {
    let mut subject = AiObjectPolicySubject::new(actor_id, actor_permissions.to_vec());
    subject.department_id = actor_department_id.map(str::to_string);
    subject
}
