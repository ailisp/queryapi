# Query API Node SQS Consumer on AWS using Serverless Framework
Consume indexer queue that has been populated by queryapi-mvp/queue-handler-alertexer.
`handler.js` is the entry point for the Lambda function.
`indexer.js` has the main indexing logic.


## VM2 Sandbox notes
https://www.npmjs.com/package/vm2
`const vm = new VM({sandbox: {changeableObject: {}}});` sandbox values allow full mutability of the object

The last statement in the sandboxed code is returned. We are using a passed in object for the return value but have the
option of returning it instead.
```
const modifiedFunction = functions[key] + ';mutationsReturnValue;';
const vmResult = vm.run(modifiedFunction);
console.log(vmResult);
```

**Security Testing for Modifiability**
Indexer function contains `mutationsReturnValue['hack'] = function() {return 'bad'}`
`console.log(vmResult.hack);`
The `freeze` function is used to prevent modification of the injected objects.



## Serverless framework SQS template

This template defines one function `producer` and one Lift construct - `jobs`. The producer function is triggered by `http` event type, accepts JSON payloads and sends it to a SQS queue for asynchronous processing. The SQS queue is created by the `jobs` queue construct of the Lift plugin. The queue is set up with a "dead-letter queue" (to receive failed messages) and a `worker` Lambda function that processes the SQS messages.

To learn more:

- about `http` event configuration options, refer to [http event docs](https://www.serverless.com/framework/docs/providers/aws/events/apigateway/)
- about the `queue` construct, refer to [the `queue` documentation in Lift](https://github.com/getlift/lift/blob/master/docs/queue.md)
- about the Lift plugin in general, refer to [the Lift project](https://github.com/getlift/lift)
- about SQS processing with AWS Lambda, please refer to the official [AWS documentation](https://docs.aws.amazon.com/lambda/latest/dg/with-sqs.html)

### Deployment
```
sls deploy --stage dev --aws-profile serverless-deploy`
```

After running deploy, you should see output similar to:

```bash
Deploying aws-node-sqs-worker-project to stage dev (us-east-1)

✔ Service deployed to stack aws-node-sqs-worker-project-dev (175s)

endpoint: POST - https://xxxxxxxxxx.execute-api.us-east-1.amazonaws.com/produce
functions:
  producer: aws-node-sqs-worker-project-dev-producer (167 kB)
  jobsWorker: aws-node-sqs-worker-project-dev-jobsWorker (167 kB)
jobs: https://sqs.us-east-1.amazonaws.com/000000000000/aws-node-sqs-worker-project-dev-jobs
```


_Note_: In current form, after deployment, your API is public and can be invoked by anyone. For production deployments, you might want to configure an authorizer. For details on how to do that, refer to [http event docs](https://www.serverless.com/framework/docs/providers/aws/events/apigateway/).

### Invocation

After successful deployment, you can now call the created API endpoint with `POST` request to invoke `producer` function:

```bash
curl --request POST 'https://wfz6gyheai.execute-api.us-west-2.amazonaws.com/produce' --header 'Content-Type: application/json' --data-raw '{"name": "John"}'
```

In response, you should see output similar to:

```bash
{"message": "Message accepted!"}
```
