mod mock;

use std::collections::HashMap;

use serde::Deserialize;

pub use self::mock::mock_workflow;

/// A deserializable workflow representation for the [`MockEngine`](super::MockEngine).
#[derive(Debug, Clone, Deserialize)]
pub struct Workflow {
    pub playbooks: HashMap<String, Playbook>,
    pub run_commands: bool,
}

/// A playbook in a [`Workflow`].
///
/// It contains multiple phases and the intents to trigger that workflow.
#[derive(Debug, Clone, Deserialize)]
pub struct Playbook {
    /// The name of this playbook
    pub name: String,
    /// The intents that trigger this playbook
    pub intents: Vec<String>,
    /// The phases in this playbook
    pub phases: Vec<Phase>,
}

/// A [`Workflow`] phase.
///
/// It contains the steps to be executed in that phase.
#[derive(Debug, Clone, Deserialize)]
pub struct Phase {
    /// 1-indexed phase number
    pub phase: u64,
    /// Display name of the phase
    pub name: String,
    /// What this phase accomplishes
    pub description: String,
    /// Duration of this phase in seconds
    pub duration_minutes: Option<u64>,
    /// Conclusion of this phase
    pub conclusion: Option<String>,
    /// Steps to be executed in this phase
    pub steps: Option<Vec<Step>>,
    /// The root cause of a problem.
    ///
    /// Present only in root cause phase
    pub root_cause: Option<RootCause>,
    /// The verification summary output
    ///
    /// Present only in verification phase
    pub verification_summary: Option<HashMap<String, String>>,
}

/// A step within a [`Phase`]
#[derive(Debug, Clone, Deserialize)]
pub struct Step {
    /// Global step number
    pub step: u32,
    /// What the agent is about to do
    pub action: String,
    /// Shell command to execute
    pub command: String,
    /// Command output
    pub output: String,
    /// Agent's reasoning about the result
    pub analysis: String,
}

/// The root cause of a problem.
#[derive(Debug, Clone, Deserialize)]
pub struct RootCause {
    /// Technical description of the issue
    pub issue: String,
    /// User-facing impact statement
    pub impact: String,
    /// Classification
    pub drift_type: String,
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_deserialize_mock_workflow() {
        let workflow: Workflow =
            serde_json::from_str(MOCK_WORKFLOW).expect("Failed to deserialize workflow");

        let playbook = workflow
            .playbooks
            .get("wordpress-troubleshooting")
            .expect("Playbook not found");

        assert!(workflow.run_commands);

        assert_eq!(
            playbook.name,
            "WordPress Database Connection Issue Troubleshooting"
        );
        assert_eq!(playbook.intents.len(), 3);
        assert_eq!(playbook.phases.len(), 6);
        assert_eq!(playbook.phases[0].name, "Symptom Verification");
        assert_eq!(playbook.phases[1].steps.as_ref().unwrap().len(), 4);
        assert_eq!(
            playbook.phases[3].root_cause.as_ref().unwrap().issue,
            "Missing security group ingress rule on RDS security group"
        );
    }

    const MOCK_WORKFLOW: &str = r#"
{
  "run_commands": true,
  "playbooks": {
    "wordpress-troubleshooting": {
      "name": "WordPress Database Connection Issue Troubleshooting",
      "intents": [
        "'Error establishing a database connection' in WordPress deployed on AWS using Terraform",
        "Identify infrastructure drift causing connectivity issues between WordPress application and RDS database",
        "Restore full functionality of WordPress site by fixing security group misconfigurations"
      ],
      "phases": [
        {
          "phase": 1,
          "name": "Symptom Verification",
          "description": "Verifying the reported issue and understand the user-facing impact",
          "steps": [
            {
              "step": 1,
              "action": "Get ALB URL from Terraform outputs",
              "command": "tofu output -raw alb_url",
              "output": "http://infraware-demo-alb-1615759059.us-east-1.elb.amazonaws.com",
              "analysis": "Retrieved the application load balancer URL for testing"
            },
            {
              "step": 2,
              "action": "Check HTTP response headers",
              "command": "curl -I http://infraware-demo-alb-1615759059.us-east-1.elb.amazonaws.com",
              "output": "HTTP/1.1 500 Internal Server Error\nDate: Fri, 30 Jan 2026 14:04:29 GMT\nContent-Type: text/html; charset=UTF-8\nConnection: keep-alive\nServer: Apache/2.4.66 (Debian)\nX-Powered-By: PHP/8.3.30",
              "analysis": "Application returns HTTP 500 error, indicating a server-side problem. Apache and PHP are running."
            },
            {
              "step": 3,
              "action": "Get error page content",
              "command": "curl -s http://infraware-demo-alb-1615759059.us-east-1.elb.amazonaws.com | grep -A 5 '<h1>'",
              "output": "<div class=\"wp-die-message\"><h1>Error establishing a database connection</h1></div>",
              "analysis": "SYMPTOM CONFIRMED: WordPress cannot connect to the database. This is the user-facing error."
            }
          ]
        },
        {
          "phase": 2,
          "name": "Infrastructure Health Check",
          "description": "Verify that underlying infrastructure components (EC2, RDS, ALB) are operational",
          "steps": [
            {
              "step": 4,
              "action": "Check EC2 instance health",
              "command": "aws ec2 describe-instances --filters \"Name=tag:Name,Values=infraware-demo-asg-instance\" \"Name=instance-state-name,Values=running\" --query \"Reservations[*].Instances[*].[InstanceId,State.Name,PrivateIpAddress,PublicIpAddress,Placement.AvailabilityZone]\" --output table",
              "output": "---------------------------------------------------------------------------------\n|                               DescribeInstances                               |\n+----------------------+----------+-------------+----------------+--------------+\n|  i-05e501b98b2b03faa |  running |  10.0.2.187 |  3.92.133.17   |  us-east-1b  |\n|  i-0eb2f0fa558269310 |  running |  10.0.1.25  |  100.48.217.87 |  us-east-1a  |\n+----------------------+----------+-------------+----------------+--------------+",
              "analysis": "Both EC2 instances are running in different availability zones (us-east-1a, us-east-1b). No EC2 instance failures."
            },
            {
              "step": 5,
              "action": "Check RDS database health",
              "command": "aws rds describe-db-instances --db-instance-identifier infraware-demo-db-2026012914533617370000000a --query 'DBInstances[0].[DBInstanceIdentifier,DBInstanceStatus,Engine,EngineVersion,Endpoint.Address,Endpoint.Port,MultiAZ]' --output table",
              "output": "-------------------------------------------------------------------------------------------\n|                                   DescribeDBInstances                                   |\n+-----------------------------------------------------------------------------------------+\n|  infraware-demo-db-2026012914533617370000000a                                           |\n|  available                                                                              |\n|  mysql                                                                                  |\n|  8.0.43                                                                                 |\n|  infraware-demo-db-2026012914533617370000000a.c6f6u8seajik.us-east-1.rds.amazonaws.com  |\n|  3306                                                                                   |\n|  True                                                                                   |",
              "analysis": "RDS database is available and healthy. MySQL 8.0.43 running with Multi-AZ enabled. Database endpoint is reachable."
            },
            {
              "step": 6,
              "action": "Get target group ARN",
              "command": "ASG_NAME=$(tofu output -raw autoscaling_group_name) && aws autoscaling describe-auto-scaling-groups --auto-scaling-group-names $ASG_NAME --query 'AutoScalingGroups[0].TargetGroupARNs[0]' --output text",
              "output": "arn:aws:elasticloadbalancing:us-east-1:961624805440:targetgroup/app-20260129145324320600000004/7cfcef6b3eaeb390",
              "analysis": "Retrieved target group ARN for health check inspection"
            },
            {
              "step": 7,
              "action": "Check ALB target health",
              "command": "aws elbv2 describe-target-health --target-group-arn arn:aws:elasticloadbalancing:us-east-1:961624805440:targetgroup/app-20260129145324320600000004/7cfcef6b3eaeb390 --query 'TargetHealthDescriptions[*].[Target.Id,TargetHealth.State,TargetHealth.Reason,TargetHealth.Description]' --output table",
              "output": "---------------------------------------------------------------------------------------------------------------------\n|                                               DescribeTargetHealth                                                |\n+---------------------+------------+-------------------------------+------------------------------------------------+\n|  i-0eb2f0fa558269310|  unhealthy |  Target.ResponseCodeMismatch  |  Health checks failed with these codes: [500]  |\n|  i-0c4d61ca122be6298|  unhealthy |  Target.FailedHealthChecks    |  Health checks failed                          |\n+---------------------+------------+-------------------------------+------------------------------------------------+",
              "analysis": "CRITICAL FINDING: All targets are unhealthy. One returning 500 errors, other failing health checks. Infrastructure is running but application is failing."
            }
          ],
          "conclusion": "Infrastructure (EC2, RDS) is healthy, but application layer is failing. Issue is not hardware/instance failure."
        },
        {
          "phase": 3,
          "name": "Security Group Analysis",
          "description": "Investigate network security configuration that might block database connectivity",
          "duration_minutes": 5,
          "steps": [
            {
              "step": 8,
              "action": "Get security group IDs from Terraform",
              "command": "RDS_SG_ID=$(tofu output -raw rds_security_group_id) && APP_SG_ID=$(tofu output -raw app_security_group_id) && echo \"RDS Security Group: $RDS_SG_ID\" && echo \"App Security Group: $APP_SG_ID\"",
              "output": "RDS Security Group: sg-0755b9193542562b8\nApp Security Group: sg-0d600f4e85840ae31",
              "analysis": "Retrieved security group IDs for network security analysis"
            },
            {
              "step": 9,
              "action": "Check RDS security group ingress rules",
              "command": "aws ec2 describe-security-groups --group-ids sg-0755b9193542562b8 --query 'SecurityGroups[0].IpPermissions' --output json",
              "output": "[]",
              "analysis": "CRITICAL FINDING: RDS security group has ZERO ingress rules! No traffic is allowed to reach the database."
            },
            {
              "step": 10,
              "action": "Check app security group for comparison",
              "command": "aws ec2 describe-security-groups --group-ids sg-0d600f4e85840ae31 --query 'SecurityGroups[0].[GroupId,GroupName,IpPermissions]' --output json",
              "output": "[\n    \"sg-0d600f4e85840ae31\",\n    \"infraware-demo-app-sg20260129145328696800000008\",\n    [\n        {\n            \"IpProtocol\": \"tcp\",\n            \"FromPort\": 80,\n            \"ToPort\": 80,\n            \"UserIdGroupPairs\": [\n                {\n                    \"Description\": \"HTTP from ALB\",\n                    \"UserId\": \"961624805440\",\n                    \"GroupId\": \"sg-039a2acf0b7abb002\"\n                }\n            ]\n        },\n        {\n            \"IpProtocol\": \"tcp\",\n            \"FromPort\": 22,\n            \"ToPort\": 22,\n            \"IpRanges\": [\n                {\n                    \"Description\": \"SSH from anywhere (demo only)\",\n                    \"CidrIp\": \"0.0.0.0/0\"\n                }\n            ]\n        }\n    ]\n]",
              "analysis": "App security group has proper ingress rules (HTTP from ALB, SSH). Confirms the RDS security group is misconfigured."
            },
            {
              "step": 11,
              "action": "Verify RDS security group egress rules",
              "command": "aws ec2 describe-security-groups --group-ids sg-0755b9193542562b8 --query 'SecurityGroups[0].IpPermissionsEgress' --output json",
              "output": "[\n    {\n        \"IpProtocol\": \"-1\",\n        \"IpRanges\": [\n            {\n                \"Description\": \"Allow all outbound traffic\",\n                \"CidrIp\": \"0.0.0.0/0\"\n            }\n        ]\n    }\n]",
              "analysis": "Egress rules exist and are correct. Problem is specifically missing ingress rule."
            },
            {
              "step": 12,
              "action": "Review Terraform configuration for expected state",
              "command": "grep -A 10 'resource \"aws_security_group\" \"rds\"' security_groups.tf",
              "output": "resource \"aws_security_group\" \"rds\" {\n  name_prefix = \"${var.project_name}-rds-sg\"\n  description = \"Security group for RDS database\"\n  vpc_id      = aws_vpc.main.id\n\n  ingress {\n    description     = \"MySQL from application instances\"\n    from_port       = 3306\n    to_port         = 3306\n    protocol        = \"tcp\"\n    security_groups = [aws_security_group.app.id]\n  }",
              "analysis": "Terraform expects ingress rule allowing TCP port 3306 from app security group. INFRASTRUCTURE DRIFT DETECTED."
            }
          ],
          "conclusion": "ROOT CAUSE IDENTIFIED: RDS security group missing MySQL ingress rule (port 3306 from app SG). This is infrastructure drift - Terraform defines the rule but it doesn't exist in AWS."
        },
        {
          "phase": 4,
          "name": "Root Cause Documentation",
          "description": "Document the identified root cause",
          "duration_minutes": 1,
          "root_cause": {
            "issue": "Missing security group ingress rule on RDS security group",
            "impact": "Application instances cannot establish TCP connections to RDS database on port 3306, resulting in 'Error establishing a database connection' for all users",
            "drift_type": "Infrastructure drift - AWS state does not match Terraform configuration"
          }
        },
        {
          "phase": 5,
          "name": "Remediation",
          "description": "Fix the issue by adding the missing security group rule",
          "duration_minutes": 2,
          "steps": [
            {
              "step": 13,
              "action": "Get AWS account ID",
              "command": "aws sts get-caller-identity --query Account --output text",
              "output": "961624805440",
              "analysis": "Retrieved AWS account ID required for security group rule creation"
            },
            {
              "step": 14,
              "action": "Add MySQL ingress rule to RDS security group",
              "command": "aws ec2 authorize-security-group-ingress --group-id sg-0755b9193542562b8 --protocol tcp --port 3306 --source-group sg-0d600f4e85840ae31 --group-owner 961624805440",
              "output": "{\n    \"Return\": true,\n    \"SecurityGroupRules\": [\n        {\n            \"SecurityGroupRuleId\": \"sgr-0d4935f525729bcd5\",\n            \"GroupId\": \"sg-0755b9193542562b8\",\n            \"GroupOwnerId\": \"961624805440\",\n            \"IsEgress\": false,\n            \"IpProtocol\": \"tcp\",\n            \"FromPort\": 3306,\n            \"ToPort\": 3306,\n            \"ReferencedGroupInfo\": {\n                \"GroupId\": \"sg-0d600f4e85840ae31\",\n                \"UserId\": \"961624805440\"\n            }\n        }\n    ]\n}",
              "analysis": "SUCCESS: Security group rule added. Rule ID: sgr-0d4935f525729bcd5. Network connectivity should be restored immediately."
            }
          ]
        },
        {
          "phase": 6,
          "name": "Verification",
          "description": "Verify the fix worked at multiple layers",
          "duration_minutes": 5,
          "steps": [
            {
              "step": 15,
              "action": "Verify security group rule persistence",
              "command": "aws ec2 describe-security-groups --group-ids sg-0755b9193542562b8 --query 'SecurityGroups[0].IpPermissions' --output json",
              "output": "[\n    {\n        \"IpProtocol\": \"tcp\",\n        \"FromPort\": 3306,\n        \"ToPort\": 3306,\n        \"UserIdGroupPairs\": [\n            {\n                \"UserId\": \"961624805440\",\n                \"GroupId\": \"sg-0d600f4e85840ae31\"\n            }\n        ],\n        \"IpRanges\": [],\n        \"Ipv6Ranges\": [],\n        \"PrefixListIds\": []\n    }\n]",
              "analysis": "VERIFIED: Security group rule is present and persisted correctly"
            },
            {
              "step": 16,
              "action": "Test WordPress HTTP response (immediate)",
              "command": "curl -I http://infraware-demo-alb-1615759059.us-east-1.elb.amazonaws.com",
              "output": "HTTP/1.1 302 Found\nDate: Fri, 30 Jan 2026 14:08:41 GMT\nContent-Type: text/html; charset=UTF-8\nConnection: keep-alive\nServer: Apache/2.4.66 (Debian)\nX-Powered-By: PHP/8.3.30\nLocation: http://infraware-demo-alb-1615759059.us-east-1.elb.amazonaws.com/wp-admin/install.php",
              "analysis": "VERIFIED: Application responding with HTTP 302 (redirect to WordPress installation). Database connection working! Changed from 500 error to successful redirect."
            },
            {
              "step": 17,
              "action": "Check ALB target health after 30 seconds",
              "command": "sleep 30 && aws elbv2 describe-target-health --target-group-arn arn:aws:elasticloadbalancing:us-east-1:961624805440:targetgroup/app-20260129145324320600000004/7cfcef6b3eaeb390 --query 'TargetHealthDescriptions[*].[Target.Id,TargetHealth.State,TargetHealth.Reason]' --output table",
              "output": "--------------------------------------------\n|           DescribeTargetHealth           |\n+----------------------+-----------+-------+\n|  i-0eb2f0fa558269310 |  healthy  |  None |\n|  i-0c4d61ca122be6298 |  healthy  |  None |\n+----------------------+-----------+-------+",
              "analysis": "VERIFIED: All ALB targets are now healthy. Health checks passing after fix. Recovery complete."
            },
            {
              "step": 18,
              "action": "Final application verification",
              "command": "curl -I http://infraware-demo-alb-1615759059.us-east-1.elb.amazonaws.com",
              "output": "HTTP/1.1 302 Found\nServer: Apache/2.4.66 (Debian)\nLocation: http://infraware-demo-alb-1615759059.us-east-1.elb.amazonaws.com/wp-admin/install.php",
              "analysis": "VERIFIED: WordPress application fully functional. Redirecting to installation page indicates database connectivity is working correctly."
            }
          ],
          "verification_summary": {
            "security_group_rule": "PASS - Rule present and correct",
            "database_connectivity": "PASS - WordPress connects to database",
            "application_response": "PASS - HTTP 302 redirect (was 500 error)",
            "alb_target_health": "PASS - 2/2 targets healthy",
            "user_facing_impact": "RESOLVED - No more database error"
          }
        }
      ]
    }
  }
}

"#;
}
